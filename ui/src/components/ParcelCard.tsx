import {
  Archive,
  Box,
  CheckCircle2,
  CircleDashed,
  Clock3,
  MapPin,
  RefreshCw,
  RotateCcw,
  TriangleAlert,
  Truck,
} from "lucide-react";
import type { Parcel, ParcelStatus } from "../types";
import { fetchLabels, fetchMessages, formatRelative, progressIndex, statusLabels, statusMessages } from "../format";
import { CarrierLogo } from "./CarrierLogo";

const statusIcons: Record<ParcelStatus, typeof Box> = {
  unknown: CircleDashed,
  announced: Box,
  in_transit: Truck,
  out_for_delivery: Truck,
  ready_for_pickup: MapPin,
  delivery_attempted: TriangleAlert,
  exception: TriangleAlert,
  delivered: CheckCircle2,
  returning: RotateCcw,
  returned: RotateCcw,
  cancelled: TriangleAlert,
};

interface Props {
  parcel: Parcel;
  onOpen: (id: string) => void;
  onRefresh: (id: string) => void;
  onArchive: (id: string) => void;
  onRestore: (id: string) => void;
}

export function ParcelCard({ parcel, onOpen, onRefresh, onArchive, onRestore }: Props) {
  const Icon = statusIcons[parcel.status];
  const progress = progressIndex(parcel.status);
  const latestEvent = [...parcel.events]
    .sort((a, b) => (Date.parse(b.timestamp ?? "") || 0) - (Date.parse(a.timestamp ?? "") || 0))[0];
  const fetchLabel = fetchLabels[parcel.fetch_state];
  const archived = Boolean(parcel.archived_at);

  return (
    <article className={`parcel-card status-${parcel.status}`}>
      <span className="card-screw screw-nw" /><span className="card-screw screw-ne" /><span className="card-screw screw-sw" /><span className="card-screw screw-se" />
      <div className="card-topline">
        <CarrierLogo carrier={parcel.carrier} />
        <code>{parcel.tracking_number}</code>
      </div>
      <div className="card-heading">
        <div className="card-title-wrap">
          <span className="status-icon" aria-hidden="true"><Icon size={19} strokeWidth={2.2} /></span>
          <div>
            <h3>{parcel.name}</h3>
          </div>
        </div>
        <span className={`status-pill tone-${parcel.status}`}>{statusLabels[parcel.status]}</span>
      </div>

      <div className="card-primary">
        <span>{statusMessages[latestEvent?.status ?? parcel.status]}</span>
      </div>

      {progress >= 0 ? (
        <div className="progress-track" data-progress={progress} aria-label={`Progress: ${statusLabels[parcel.status]}`}>
          {[parcel.carrier === "hermes_de" ? "Announced" : "Picked up", "In transit", parcel.status === "ready_for_pickup" ? "Pickup" : "Out for delivery", "Delivered"].map((label, index) => (
            <div className={`progress-step ${index <= progress ? "complete" : ""} ${index === progress ? "current" : ""}`} key={label}>
              <span className="progress-dot" />
              <span>{label}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className="attention-strip"><TriangleAlert size={16} /> Follow the carrier message in details</div>
      )}

      <div className="card-meta">
        <span><Clock3 size={14} /> Checked {formatRelative(parcel.last_checked_at)}</span>
        {fetchLabel && <span className={`fetch-label fetch-${parcel.fetch_state}`}>{fetchLabel}</span>}
      </div>
      {fetchMessages[parcel.fetch_state] && <p className="inline-error">{fetchMessages[parcel.fetch_state]}</p>}

      <div className="card-actions">
        <button className="button-link" onClick={() => onOpen(parcel.id)}>View details</button>
        <div>
          {!archived && !["fetching", "queued"].includes(parcel.fetch_state) && (
            <button className="icon-button" title="Refresh package" aria-label={`Refresh ${parcel.name}`} onClick={() => onRefresh(parcel.id)}>
              <RefreshCw size={17} />
            </button>
          )}
          <button
            className="icon-button"
            title={archived ? "Restore package" : "Archive package"}
            aria-label={`${archived ? "Restore" : "Archive"} ${parcel.name}`}
            onClick={() => archived ? onRestore(parcel.id) : onArchive(parcel.id)}
          >
            {archived ? <RotateCcw size={17} /> : <Archive size={17} />}
          </button>
        </div>
      </div>
    </article>
  );
}
