import { useEffect, useRef, useState } from "react";
import { Bell, MonitorUp, ShieldCheck, X } from "lucide-react";
import type { Settings, SourceHealth } from "../types";
import { formatRelative } from "../format";

interface Props {
  open: boolean;
  settings: Settings;
  sources: SourceHealth[];
  onClose: () => void;
  onSave: (settings: Settings) => Promise<void>;
}

export function SettingsDialog({ open, settings, sources, onClose, onSave }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [draft, setDraft] = useState(settings);
  const [saving, setSaving] = useState(false);

  useEffect(() => setDraft(settings), [settings]);
  useEffect(() => {
    if (open && !ref.current?.open) ref.current?.showModal();
    if (!open && ref.current?.open) ref.current.close();
  }, [open]);

  const save = async () => {
    setSaving(true);
    try { await onSave(draft); onClose(); } finally { setSaving(false); }
  };

  const toggle = (key: keyof Settings) => setDraft((value) => ({ ...value, [key]: !value[key] }));

  return (
    <dialog ref={ref} className="modal settings-modal" onCancel={(event) => { event.preventDefault(); onClose(); }} onClose={onClose}>
      <header className="modal-header">
        <div><span className="eyebrow">Preferences</span><h2>Settings</h2></div>
        <button className="icon-button" onClick={onClose} aria-label="Close settings"><X size={19} /></button>
      </header>
      <div className="modal-body settings-body">
        <section>
          <h3><MonitorUp size={18} /> Appearance & startup</h3>
          <label>Theme
            <select value={draft.theme} onChange={(event) => setDraft({ ...draft, theme: event.target.value as Settings["theme"] })}>
              <option value="system">Follow Windows</option><option value="light">Light</option><option value="dark">Dark</option>
            </select>
          </label>
          <label className="setting-row"><span><strong>Start with Windows</strong><small>Launch Walle after you sign in</small></span><input type="checkbox" checked={draft.startWithWindows} onChange={() => toggle("startWithWindows")} /></label>
          <label className="setting-row"><span><strong>Start minimized</strong><small>Open quietly in the system tray</small></span><input type="checkbox" checked={draft.startMinimized} onChange={() => toggle("startMinimized")} /></label>
        </section>
        <section>
          <h3><Bell size={18} /> Notifications</h3>
          {([
            ["notifyDelivered", "Delivered", "When a parcel is successfully delivered"],
            ["notifyPickup", "Ready for pickup", "When collection is required"],
            ["notifyProblems", "Problems & returns", "Attempts, delays, exceptions, and returns"],
            ["notifyOutForDelivery", "Out for delivery", "Optional alert when the final route starts"],
          ] as const).map(([key, title, copy]) => (
            <label className="setting-row" key={key}><span><strong>{title}</strong><small>{copy}</small></span><input type="checkbox" checked={draft[key]} onChange={() => toggle(key)} /></label>
          ))}
        </section>
        <section>
          <h3><ShieldCheck size={18} /> Carrier sources</h3>
          <div className="source-list">
            {sources.map((source) => (
              <article key={source.carrier}>
                <div><strong>{source.label}</strong><span className={source.enabled ? "health-good" : "health-paused"}>{source.enabled ? "Available" : "Paused"}</span></div>
                <p>{source.limitation}</p>
                <small>{source.parser_version} · Last success {formatRelative(source.last_success_at)}</small>
              </article>
            ))}
          </div>
          <p className="privacy-note">Package metadata stays on this computer. A refresh sends the tracking number and any required postcode directly to the selected carrier.</p>
        </section>
      </div>
      <footer className="modal-footer"><button className="button-secondary" onClick={onClose}>Cancel</button><button className="button-primary" disabled={saving} onClick={() => void save()}>{saving ? "Saving…" : "Save settings"}</button></footer>
    </dialog>
  );
}
