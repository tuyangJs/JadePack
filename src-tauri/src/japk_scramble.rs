//! JAPK 可逆混淆 v2：动态密钥 + 多层混淆 + 位置相关变换
//! 与 JadeView `japk_scramble.rs` 须保持完全一致。

use sha2::{Digest, Sha256};

pub const SCRAMBLE_MAGIC: &[u8; 8] = b"JPKBIN02";
pub const SCRAMBLE_VERSION: u8 = 0x02;

const HEADER_SIZE: usize = 8 + 1 + 32;
const FOOTER_SIZE: usize = 4;

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn derive_keys(content_hash: &[u8; 32]) -> ([u8; 32], [u8; 8], [u8; 8]) {
    let mut k1 = [0u8; 32];
    let mut k2 = [0u8; 8];
    let mut k3 = [0u8; 8];
    
    for i in 0..32 {
        k1[i] = content_hash[i]
            .wrapping_add(0x5A)
            .wrapping_mul(0x37)
            .wrapping_add((i as u8).wrapping_mul(0x13));
    }
    
    for i in 0..8 {
        k2[i] = (content_hash[i]
            .wrapping_add(content_hash[i + 8])
            .wrapping_mul(0x7F))
            ^ content_hash[i + 16];
        k3[i] = (content_hash[i + 24]
            .wrapping_mul(0x3D)
            .wrapping_add(content_hash[i]))
            ^ 0xA5;
    }
    
    (k1, k2, k3)
}

fn build_permutation_table(k2: &[u8; 8]) -> [usize; 8] {
    let mut table = [0usize; 8];
    let mut indices: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
    
    for i in 0..8 {
        let j = (k2[i] as usize) % (8 - i);
        table[i] = indices[j] as usize;
        indices.swap(j, 7 - i);
    }
    
    table
}

fn apply_xor_layer(data: &mut [u8], k1: &[u8; 32]) {
    for (i, b) in data.iter_mut().enumerate() {
        let pos_key = k1[i % 32];
        let offset_key = k1[(i.wrapping_add(17)) % 32];
        *b = (*b ^ pos_key) ^ offset_key.wrapping_add((i >> 3) as u8);
    }
}

fn apply_byte_permutation(data: &mut [u8], table: &[usize; 8]) {
    for chunk in data.chunks_exact_mut(8) {
        let original: [u8; 8] = [chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]];
        for (i, &new_pos) in table.iter().enumerate() {
            chunk[i] = original[new_pos];
        }
    }
}

fn apply_bit_rotation(data: &mut [u8], k3: &[u8; 8]) {
    for (i, b) in data.iter_mut().enumerate() {
        let rotation = (k3[i % 8] % 8) as u32;
        *b = b.rotate_left(rotation);
    }
}

fn reverse_bit_rotation(data: &mut [u8], k3: &[u8; 8]) {
    for (i, b) in data.iter_mut().enumerate() {
        let rotation = (k3[i % 8] % 8) as u32;
        *b = b.rotate_right(rotation);
    }
}

fn reverse_byte_permutation(data: &mut [u8], table: &[usize; 8]) {
    let mut reverse_table = [0usize; 8];
    for (i, &pos) in table.iter().enumerate() {
        reverse_table[pos] = i;
    }
    
    for chunk in data.chunks_exact_mut(8) {
        let original: [u8; 8] = [chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]];
        for (i, &new_pos) in reverse_table.iter().enumerate() {
            chunk[i] = original[new_pos];
        }
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

pub fn peek_is_scrambled(file_start: &[u8]) -> bool {
    file_start.len() >= 8 && file_start[..8] == SCRAMBLE_MAGIC[..]
}

pub fn wrap_asar_for_disk(plain_asar: &[u8]) -> Vec<u8> {
    let content_hash = sha256(plain_asar);
    let (k1, k2, k3) = derive_keys(&content_hash);
    let perm_table = build_permutation_table(&k2);
    
    let mut body = plain_asar.to_vec();
    
    apply_xor_layer(&mut body, &k1);
    apply_byte_permutation(&mut body, &perm_table);
    apply_bit_rotation(&mut body, &k3);
    
    let checksum = crc32(&body);
    
    let mut out = Vec::with_capacity(HEADER_SIZE + body.len() + FOOTER_SIZE);
    out.extend_from_slice(SCRAMBLE_MAGIC.as_slice());
    out.push(SCRAMBLE_VERSION);
    out.extend_from_slice(&content_hash);
    out.extend_from_slice(&body);
    out.extend_from_slice(&checksum.to_le_bytes());
    
    out
}

pub fn into_plain_asar_bytes(raw: Vec<u8>) -> Result<Vec<u8>, String> {
    if !peek_is_scrambled(&raw) {
        return Ok(raw);
    }
    
    let min_size = HEADER_SIZE + FOOTER_SIZE;
    if raw.len() < min_size {
        return Err("混淆包过短".to_string());
    }
    
    let version = raw[8];
    if version != SCRAMBLE_VERSION {
        return Err(format!("不支持的混淆版本: {}", version));
    }
    
    let mut content_hash = [0u8; 32];
    content_hash.copy_from_slice(&raw[9..41]);
    
    let body_len = raw.len() - FOOTER_SIZE;
    let stored_checksum = u32::from_le_bytes([raw[body_len], raw[body_len + 1], raw[body_len + 2], raw[body_len + 3]]);
    
    let body = &raw[HEADER_SIZE..body_len];
    let computed_checksum = crc32(body);
    
    if stored_checksum != computed_checksum {
        return Err("混淆包校验失败".to_string());
    }
    
    let (k1, k2, k3) = derive_keys(&content_hash);
    let perm_table = build_permutation_table(&k2);
    
    let mut data = body.to_vec();
    
    reverse_bit_rotation(&mut data, &k3);
    reverse_byte_permutation(&mut data, &perm_table);
    apply_xor_layer(&mut data, &k1);
    
    Ok(data)
}
