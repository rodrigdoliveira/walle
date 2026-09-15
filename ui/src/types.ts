export type Carrier = "dhl_paket_de" | "hermes_de";

export type ParcelStatus =
  | "unknown"
  | "announced"
  | "in_transit"
  | "out_for_delivery"
  | "ready_for_pickup"
  | "delivery_attempted"
  | "exception"
  | "delivered"
  | "returning"
  | "returned"
  | "cancelled";

export type FetchState =
  | "idle"
  | "queued"
  | "fetching"
  | "not_found"
  | "needs_input"
  | "offline"
  | "rate_limited"
  | "access_denied"
  | "source_changed"
  | "source_error";

export interface TrackingEvent {
  source_id: string | null;
  raw_status: string | null;
  status: ParcelStatus;
  description: string;
  location: string | null;
  timestamp: string | null;
}

export interface DeliveryEstimate {
  from: string | null;
  until: string | null;
  date: string | null;
}

export interface Parcel {
  id: string;
  name: string;
  tracking_number: string;
  carrier: Carrier;
  destination_country: string | null;
  destination_postcode: string | null;
  shipment_date: string | null;
  international: boolean;
  generation: number;
  status: ParcelStatus;
  raw_status: string | null;
  summary: string | null;
  sender: string | null;
  estimate: DeliveryEstimate | null;
  international_tracking_url: string | null;
  events: TrackingEvent[];
  fetch_state: FetchState;
  last_error: string | null;
  created_at: string;
  updated_at: string;
  archived_at: string | null;
  last_checked_at: string | null;
  last_success_at: string | null;
  next_check_at: string | null;
  parser_version: string | null;
}

export interface AddParcelInput {
  name: string;
  trackingNumber: string;
  carrier: Carrier;
  destinationCountry?: string | null;
  destinationPostcode?: string | null;
  shipmentDate?: string | null;
  international: boolean;
}

export interface UpdateParcelInput {
  id: string;
  name: string;
  destinationCountry?: string | null;
  destinationPostcode?: string | null;
  shipmentDate?: string | null;
  international: boolean;
}

export type ThemePreference = "system" | "light" | "dark";

export interface Settings {
  theme: ThemePreference;
  notifyDelivered: boolean;
  notifyPickup: boolean;
  notifyProblems: boolean;
  notifyOutForDelivery: boolean;
  startWithWindows: boolean;
  startMinimized: boolean;
}

export interface SourceHealth {
  carrier: Carrier;
  label: string;
  enabled: boolean;
  parser_version: string;
  last_success_at: string | null;
  limitation: string;
}
