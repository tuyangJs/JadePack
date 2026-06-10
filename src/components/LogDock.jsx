import { useRef, useEffect, useState } from "react";
import { Button } from "@heroui/react";

function levelClass(level) {
  if (level === "success") return "log-success";
  if (level === "error") return "log-error";
  if (level === "info") return "log-info";
  if (level === "progress") return "log-progress";
  return "log-dim";
}

function parseProgressMessage(message) {
  const sepIdx = message.indexOf("|");
  if (sepIdx > 0) {
    const pct = parseInt(message.substring(0, sepIdx), 10) || 0;
    const msg = message.substring(sepIdx + 1);
    return { targetPct: Math.min(100, Math.max(0, pct)), msg };
  }
  return { targetPct: 0, msg: message };
}

function ProgressBar({ message }) {
  const { targetPct, msg } = parseProgressMessage(message);

  const [displayPct, setDisplayPct] = useState(targetPct);
  const rafRef = useRef(null);
  const creepRef = useRef(null);
  const displayRef = useRef(targetPct);

  useEffect(() => {
    if (creepRef.current) {
      clearTimeout(creepRef.current);
      creepRef.current = null;
    }

    const from = displayRef.current;
    const to = targetPct;

    if (to < from) {
      displayRef.current = 0;
      setDisplayPct(0);
    }

    const startFrom = to < from ? 0 : from;

    const startCreep = (base) => {
      if (base >= 100) return;
      const ceiling = Math.min(base + 9, 99);
      const totalSteps = ceiling - base;
      const tick = () => {
        if (displayRef.current >= ceiling) {
          creepRef.current = null;
          return;
        }
        displayRef.current += 1;
        setDisplayPct(displayRef.current);
        const stepsTaken = displayRef.current - base;
        const ratio = totalSteps > 0 ? stepsTaken / totalSteps : 1;
        const delay = 1000 + ratio * 6000 + Math.random() * (2000 + ratio * 3000);
        creepRef.current = setTimeout(tick, delay);
      };
      const delay = 1000 + Math.random() * 2000;
      creepRef.current = setTimeout(tick, delay);
    };

    if (startFrom === to) {
      startCreep(to);
      return;
    }

    const diff = to - startFrom;
    const duration = Math.min(Math.abs(diff) * 40, 1200);
    const startTime = performance.now();

    const animate = (now) => {
      const elapsed = now - startTime;
      const t = Math.min(elapsed / duration, 1);
      const eased = 1 - Math.pow(1 - t, 3);
      const current = Math.round(startFrom + diff * eased);
      displayRef.current = current;
      setDisplayPct(current);
      if (t < 1) {
        rafRef.current = requestAnimationFrame(animate);
      } else {
        startCreep(to);
      }
    };

    if (rafRef.current) cancelAnimationFrame(rafRef.current);
    rafRef.current = requestAnimationFrame(animate);

    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      if (creepRef.current) {
        clearTimeout(creepRef.current);
        creepRef.current = null;
      }
    };
  }, [targetPct]);

  const safePct = Math.max(0, Math.min(100, displayPct));
  const filled = Math.round(safePct / 5);
  const empty = 20 - filled;
  const bar = "█".repeat(filled) + "░".repeat(empty);
  const cls = safePct >= 100 ? "log-progress-bar log-progress-bar--done" : "log-progress-bar";
  return (
    <span className={cls}>
      [{bar}] {safePct}% {msg}
    </span>
  );
}

function LogDock({ logs, expanded, onToggleExpanded, onClearLogs, isBuilding }) {
  const terminalRef = useRef(null);
  const userScrolledUp = useRef(false);

  const handleScroll = () => {
    const el = terminalRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
    userScrolledUp.current = !atBottom;
  };

  const scrollToBottom = (smooth) => {
    const el = terminalRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: smooth ? "smooth" : "auto" });
  };

  useEffect(() => {
    if (!userScrolledUp.current) {
      requestAnimationFrame(() => scrollToBottom(true));
    }
  }, [logs]);

  useEffect(() => {
    userScrolledUp.current = false;
    requestAnimationFrame(() => scrollToBottom(false));
  }, [expanded]);

  const handleKeyDown = (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "a") {
      e.preventDefault();
      const el = terminalRef.current;
      if (!el) return;
      const range = document.createRange();
      range.selectNodeContents(el);
      const sel = window.getSelection();
      sel.removeAllRanges();
      sel.addRange(range);
    }
  };

  return (
    <div className="log-dock">
      <div className="log-dock-header">
        <div className="log-dock-left">
          <span className="log-dock-title">终端日志</span>
        </div>
        <div className="log-dock-actions">
          <Button size="sm" variant="light" className="log-dock-btn" onPress={onClearLogs} isDisabled={isBuilding}>
            清空
          </Button>
          <Button size="sm" variant="light" className="log-dock-btn" onPress={onToggleExpanded}>
            {expanded ? "收起" : "展开"}
          </Button>
        </div>
      </div>
      <div
        className={`log-textarea ${expanded ? "is-expanded" : "is-collapsed"}`}
        ref={terminalRef}
        onScroll={handleScroll}
        onKeyDown={handleKeyDown}
        tabIndex={0}
      >
        {logs.length === 0 ? (
          <div className="log-line log-dim">$ 暂无日志</div>
        ) : (
          logs.map((line) => (
            <div key={line.id} className={`log-line ${levelClass(line.level)}`}>
              {line.level === "progress" ? (
                <>
                  <span className="log-time">[{line.time}]</span> <ProgressBar message={line.message} />
                </>
              ) : (
                <>
                  <span className="log-time">[{line.time}]</span> {line.message}
                </>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}

export default LogDock;
