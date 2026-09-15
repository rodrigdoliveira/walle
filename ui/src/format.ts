import type { Carrier, DeliveryEstimate, FetchState, ParcelStatus } from "./types";

export const carrierLabels: Record<Carrier, string> = {
  dhl_paket_de: "DHL",
  hermes_de: "Hermes",
};

export const statusLabels: Record<ParcelStatus, string> = {
  unknown: "Waiting for update",
  announced: "Announced",
  in_transit: "In transit",
  out_for_delivery: "Out for delivery",
  ready_for_pickup: "Ready for pickup",
  delivery_attempted: "Delivery attempted",
  exception: "Needs attention",
  delivered: "Delivered",
  returning: "Returning",
  returned: "Returned",
  cancelled: "Cancelled",
};

export const fetchLabels: Partial<Record<FetchState, string>> = {
  queued: "Update queued",
  fetching: "Updating…",
  not_found: "No tracking updates yet",
  needs_input: "More information required",
  offline: "Offline",
  rate_limited: "Carrier cooldown",
  access_denied: "Carrier access denied",
  source_changed: "Carrier page changed",
  source_error: "Could not update",
};

export const fetchMessages: Partial<Record<FetchState, string>> = {
  queued: "This shipment is waiting for an update.",
  fetching: "Walle is checking the carrier now.",
  not_found: "The carrier has not published tracking information for this number yet.",
  needs_input: "Open Edit and provide the extra shipment details requested by the carrier.",
  offline: "Walle could not reach the carrier. It will try again automatically.",
  rate_limited: "The carrier asked Walle to wait before checking again.",
  access_denied: "The carrier refused this tracking request.",
  source_changed: "The carrier changed its tracking service and this update could not be read.",
  source_error: "The carrier update could not be read. The last successful status is still shown.",
};

export const terminalStatuses = new Set<ParcelStatus>(["delivered", "returned", "cancelled"]);
export const attentionStatuses = new Set<ParcelStatus>(["ready_for_pickup", "delivery_attempted", "exception", "returning"]);

export const statusMessages: Record<ParcelStatus, string> = {
  unknown: "The carrier has published a tracking update.",
  announced: "The carrier has received the shipment information.",
  in_transit: "The shipment is moving through the carrier network.",
  out_for_delivery: "The shipment is out for delivery.",
  ready_for_pickup: "The shipment is ready for pickup.",
  delivery_attempted: "A delivery attempt was made.",
  exception: "The carrier reported a problem with this shipment.",
  delivered: "The shipment was delivered.",
  returning: "The shipment is being returned to the sender.",
  returned: "The shipment was returned to the sender.",
  cancelled: "The shipment was cancelled.",
};

export function trackingMessage(description: string | null | undefined, status: ParcelStatus): string {
  return description?.trim() || statusMessages[status];
}

export function senderText(value: string): string {
  const normalized = value.trim().toLocaleLowerCase("de-DE");
  if (["privatversand", "privatsendung", "privatkunde"].includes(normalized)) {
    return "Private shipment";
  }
  if (["unbekannt", "absender unbekannt"].includes(normalized)) {
    return "Sender unavailable";
  }
  return value;
}

export function locationText(value: string): string {
  const exactTranslations: Record<string, string> = {
    deutschland: "Germany",
    österreich: "Austria",
    schweiz: "Switzerland",
    spanien: "Spain",
    frankreich: "France",
    italien: "Italy",
    niederlande: "Netherlands",
    belgien: "Belgium",
    polen: "Poland",
    tschechien: "Czechia",
  };
  const normalized = value.trim().toLocaleLowerCase("de-DE");
  if (exactTranslations[normalized]) return exactTranslations[normalized];
  if (/paketzentrum|logistikzentrum|zustellbasis|verteilzentrum/i.test(value)) {
    return "Carrier facility";
  }
  return value;
}

export function formatRelative(value: string | null): string {
  if (!value) return "Never checked";
  const timestamp = new Date(value).getTime();
  if (Number.isNaN(timestamp)) return value;
  const seconds = Math.round((timestamp - Date.now()) / 1000);
  const absolute = Math.abs(seconds);
  const formatter = new Intl.RelativeTimeFormat("en-GB", { numeric: "auto" });
  if (absolute < 60) return formatter.format(seconds, "second");
  if (absolute < 3600) return formatter.format(Math.round(seconds / 60), "minute");
  if (absolute < 86_400) return formatter.format(Math.round(seconds / 3600), "hour");
  return formatter.format(Math.round(seconds / 86_400), "day");
}

export function formatTimestamp(value: string | null): string {
  if (!value) return "Time unavailable";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en-GB", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

export function estimateText(estimate: DeliveryEstimate | null): string | null {
  if (!estimate) return null;
  if (estimate.date && estimate.from && estimate.until) {
    return `${formatTimestamp(estimate.date).split(",")[0]}, ${estimate.from}–${estimate.until}`;
  }
  if (estimate.date) return `Expected ${formatTimestamp(estimate.date).split(",")[0]}`;
  if (estimate.from && estimate.until) return `Expected ${estimate.from}–${estimate.until}`;
  return estimate.from ?? estimate.until;
}

export function progressIndex(status: ParcelStatus): number {
  switch (status) {
    case "announced": return 0;
    case "in_transit": return 1;
    case "out_for_delivery": return 2;
    case "ready_for_pickup": return 2;
    case "delivered": return 3;
    default: return -1;
  }
}
