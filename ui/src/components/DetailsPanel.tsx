import { useEffect, useRef, useState } from "react";
import {
  Archive,
  CalendarClock,
  Clipboard,
  ExternalLink,
  MapPin,
  RefreshCw,
  RotateCcw,
  Save,
  Trash2,
  X,
} from "lucide-react";
import type { Parcel, UpdateParcelInput } from "../types";
import { carrierLabels, estimateText, fetchLabels, fetchMessages, formatRelative, formatTimestamp, locationText, progressIndex, senderText, statusLabels, trackingMessage } from "../format";
import { CarrierLogo } from "./CarrierLogo";

interface Props {
  parcel: Parcel | null;
  onClose: () => void;
  onRefresh: (id: string) => Promise<void>;
  onArchive: (id: string) => Promise<void>;
  onRestore: (id: string) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onUpdate: (input: UpdateParcelInput) => Promise<void>;
  onOpenCarrier: (id: string, partner?: boolean) => Promise<void>;
}

export function DetailsPanel({ parcel, onClose, onRefresh, onArchive, onRestore, onDelete, onUpdate, onOpenCarrier }: Props) {
  const panelRef = useRef<HTMLElement>(null);
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [postcode, setPostcode] = useState("");
  const [country, setCountry] = useState("");
  const [shipmentDate, setShipmentDate] = useState("");
  const [international, setInternational] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!parcel) return;
    setName(parcel.name);
    setPostcode(parcel.destination_postcode ?? "");
    setCountry(parcel.destination_country ?? "");
    setShipmentDate(parcel.shipment_date ?? "");
    setInternational(parcel.international);
    setEditing(false);
    window.setTimeout(() => panelRef.current?.focus(), 0);
  }, [parcel?.id, parcel?.updated_at]);

  if (!parcel) return null;
  const archived = Boolean(parcel.archived_at);
  const events = [...parcel.events].sort((a, b) => (Date.parse(b.timestamp ?? "") || 0) - (Date.parse(a.timestamp ?? "") || 0));
  const estimate = estimateText(parcel.estimate);
  const summary = parcel.summary?.trim() || null;
  const progress = progressIndex(parcel.status);
  const progressLabels = [parcel.carrier === "hermes_de" ? "Announced" : "Picked up", "In transit", parcel.status === "ready_for_pickup" ? "Pickup" : "Out for delivery", "Delivered"];

  const perform = async (action: () => Promise<void>) => {
    setBusy(true);
    try { await action(); } finally { setBusy(false); }
  };

  const save = () => perform(async () => {
    await onUpdate({
      id: parcel.id,
      name,
      destinationCountry: country || null,
      destinationPostcode: postcode || null,
      shipmentDate: shipmentDate || null,
      international,
    });
    setEditing(false);
  });

  const remove = () => {
    if (!window.confirm(`Delete “${parcel.name}” and its local tracking history?`)) return;
    void perform(async () => { await onDelete(parcel.id); onClose(); });
  };

  return (
    <>
      <button className="panel-backdrop" aria-label="Close package details" onClick={onClose} />
      <aside className="details-panel" ref={panelRef} tabIndex={-1} aria-label={`Details for ${parcel.name}`}>
        <header className="details-header">
          <div>
            <span className="eyebrow">{carrierLabels[parcel.carrier]}</span>
            {editing ? <input className="title-input" value={name} maxLength={120} onChange={(event) => setName(event.target.value)} /> : <h2>{parcel.name}</h2>}
          </div>
          <button className="icon-button" onClick={onClose} aria-label="Close details"><X size={20} /></button>
        </header>

        <div className="details-scroll">
          <div className="details-identity">
            <CarrierLogo carrier={parcel.carrier} />
            <code>{parcel.tracking_number}</code>
            <button className="icon-button" title="Copy tracking number" aria-label="Copy tracking number" onClick={() => void navigator.clipboard.writeText(parcel.tracking_number)}><Clipboard size={16} /></button>
          </div>
          <section className={`status-hero tone-${parcel.status}`}>
            <span>{statusLabels[parcel.status]}</span>
            <strong>{estimate ?? summary ?? trackingMessage(events[0]?.description, parcel.status)}</strong>
          </section>

          {progress >= 0 && (
            <div className={`details-progress status-${parcel.status}`} data-progress={progress} aria-label={`Progress: ${statusLabels[parcel.status]}`}>
              {progressLabels.map((label, index) => (
                <div className={`detail-progress-step ${index <= progress ? "complete" : ""} ${index === progress ? "current" : ""}`} key={label}>
                  <i /><span>{label}</span>
                </div>
              ))}
            </div>
          )}

          {parcel.fetch_state !== "idle" && fetchLabels[parcel.fetch_state] && (
            <div className={`fetch-banner fetch-${parcel.fetch_state}`}>
              <strong>{fetchLabels[parcel.fetch_state]}</strong>
              {fetchMessages[parcel.fetch_state] && <span>{fetchMessages[parcel.fetch_state]}</span>}
            </div>
          )}

          <section className="detail-section">
            <div className="section-heading"><h3>Tracking</h3><button className="button-link" onClick={() => setEditing((value) => !value)}>{editing ? "Cancel edit" : "Edit"}</button></div>
            <div className="tracking-number"><code>{parcel.tracking_number}</code><button className="icon-button" title="Copy tracking number" onClick={() => void navigator.clipboard.writeText(parcel.tracking_number)}><Clipboard size={16} /></button></div>
            {editing ? (
              <div className="edit-grid">
                <label>Country<input maxLength={2} value={country} onChange={(event) => setCountry(event.target.value.toUpperCase())} /></label>
                <label>Recipient postcode<input maxLength={16} value={postcode} onChange={(event) => setPostcode(event.target.value)} /></label>
                <p className="field-hint span-full">
                  {parcel.carrier === "dhl_paket_de"
                    ? "DHL may use this to verify the recipient and return extra delivery details."
                    : "Hermes tracking uses the parcel number; some recipient services on the Hermes website may ask for the postcode."}
                </p>
                {parcel.carrier === "dhl_paket_de" && <label>Shipment date<input type="date" value={shipmentDate} onChange={(event) => setShipmentDate(event.target.value)} /></label>}
                <label className="check-row"><input type="checkbox" checked={international} onChange={(event) => setInternational(event.target.checked)} /> International</label>
                <button className="button-primary span-full" disabled={busy || !name.trim()} onClick={() => void save()}><Save size={16} /> Save changes</button>
              </div>
            ) : (
              <dl className="metadata-list">
                <div><dt>Last checked</dt><dd>{formatRelative(parcel.last_checked_at)}</dd></div>
                <div><dt>Last carrier update</dt><dd>{events[0] ? formatRelative(events[0].timestamp) : "No event yet"}</dd></div>
                {parcel.sender && <div><dt>Sender</dt><dd>{senderText(parcel.sender)}</dd></div>}
                {parcel.destination_country && <div><dt>Destination</dt><dd>{parcel.destination_country}</dd></div>}
                {parcel.destination_postcode && <div><dt>Recipient postcode</dt><dd>{parcel.destination_postcode}</dd></div>}
              </dl>
            )}
          </section>

          <section className="detail-section">
            <div className="section-heading"><h3>Timeline</h3><span>{events.length} {events.length === 1 ? "event" : "events"}</span></div>
            {events.length ? (
              <ol className="event-timeline">
                {events.map((event, index) => (
                  <li key={`${event.timestamp ?? "unknown"}-${event.description}-${index}`}>
                    <span className={`event-dot tone-${event.status}`} />
                    <div>
                      <strong>{trackingMessage(event.description, event.status)}</strong>
                      <span><CalendarClock size={13} /> {formatTimestamp(event.timestamp)}</span>
                      {event.location && <span><MapPin size={13} /> {locationText(event.location)}</span>}
                    </div>
                  </li>
                ))}
              </ol>
            ) : <p className="empty-copy">The carrier has not published a timeline event yet.</p>}
          </section>

          {parcel.international_tracking_url && (
            <section className="detail-section partner-card">
              <h3>International handoff</h3>
              <p>The carrier supplied a linked tracking page for the delivery partner.</p>
              <button className="button-secondary" onClick={() => void onOpenCarrier(parcel.id, true)}><ExternalLink size={16} /> Open partner page</button>
            </section>
          )}

          <section className="detail-actions">
            <button className="button-secondary" disabled={busy} onClick={() => void perform(() => onRefresh(parcel.id))}><RefreshCw size={16} /> Refresh</button>
            <button className="button-secondary" disabled={busy} onClick={() => void onOpenCarrier(parcel.id)}><ExternalLink size={16} /> Carrier website</button>
            <button className="button-secondary" disabled={busy} onClick={() => void perform(() => archived ? onRestore(parcel.id) : onArchive(parcel.id))}>
              {archived ? <RotateCcw size={16} /> : <Archive size={16} />} {archived ? "Restore" : "Archive"}
            </button>
            <button className="button-danger" disabled={busy} onClick={remove}><Trash2 size={16} /> Delete</button>
          </section>
        </div>
      </aside>
    </>
  );
}
