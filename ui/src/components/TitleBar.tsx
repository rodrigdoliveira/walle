import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export function TitleBar() {
  const run = (action: "minimize" | "maximize" | "close") => {
    if (!isTauri()) return;
    const appWindow = getCurrentWindow();
    if (action === "minimize") void appWindow.minimize();
    if (action === "maximize") void appWindow.toggleMaximize();
    if (action === "close") void appWindow.close();
  };

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-grip" data-tauri-drag-region><i /><i /><i /><i /><i /><i /></div>
      <span className="titlebar-name" data-tauri-drag-region>WALLE · PARCEL CONTROL</span>
      <div className="titlebar-controls">
        <button onClick={() => run("minimize")} aria-label="Minimize window"><Minus size={16} /></button>
        <button onClick={() => run("maximize")} aria-label="Maximize window"><Square size={13} /></button>
        <button className="titlebar-close" onClick={() => run("close")} aria-label="Hide Walle"><X size={17} /></button>
      </div>
    </header>
  );
}
