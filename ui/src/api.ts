import { invoke } from "@tauri-apps/api/core";
import type { AddParcelInput, Parcel, Settings, SourceHealth, UpdateParcelInput } from "./types";
import { mockParcels, mockSettings, mockSourceHealth } from "./mock";

const MOCK_PARCELS_KEY = "walle.mock.parcels.v2";
const MOCK_SETTINGS_KEY = "walle.mock.settings.v1";

export const isNative = () => "__TAURI_INTERNALS__" in window;

function readMockParcels(): Parcel[] {
  const saved = localStorage.getItem(MOCK_PARCELS_KEY);
  if (!saved) return structuredClone(mockParcels);
  try {
    return JSON.parse(saved) as Parcel[];
  } catch {
    return structuredClone(mockParcels);
  }
}

function writeMockParcels(parcels: Parcel[]) {
  localStorage.setItem(MOCK_PARCELS_KEY, JSON.stringify(parcels));
}

function mockParcel(id: string): Parcel {
  const parcel = readMockParcels().find((item) => item.id === id);
  if (!parcel) throw new Error("Package no longer exists");
  return parcel;
}

export async function listParcels(): Promise<Parcel[]> {
  return isNative() ? invoke<Parcel[]>("list_parcels") : readMockParcels();
}

export async function addParcel(input: AddParcelInput): Promise<Parcel> {
  if (isNative()) return invoke<Parcel>("add_parcel", { input });
  const now = new Date().toISOString();
  const parcel: Parcel = {
    id: crypto.randomUUID(),
    name: input.name.trim(),
    tracking_number: input.trackingNumber.trim(),
    carrier: input.carrier,
    destination_country: input.destinationCountry?.trim() || null,
    destination_postcode: input.destinationPostcode?.trim() || null,
    shipment_date: input.shipmentDate?.trim() || null,
    international: input.international,
    generation: 1,
    status: "unknown",
    raw_status: null,
    summary: "Waiting for first update",
    sender: null,
    estimate: null,
    international_tracking_url: null,
    events: [],
    fetch_state: "queued",
    last_error: null,
    created_at: now,
    updated_at: now,
    archived_at: null,
    last_checked_at: null,
    last_success_at: null,
    next_check_at: now,
    parser_version: null,
  };
  const parcels = readMockParcels();
  parcels.unshift(parcel);
  writeMockParcels(parcels);
  return parcel;
}

export async function updateParcel(input: UpdateParcelInput): Promise<Parcel> {
  if (isNative()) return invoke<Parcel>("update_parcel", { input });
  const parcels = readMockParcels();
  const index = parcels.findIndex((item) => item.id === input.id);
  if (index < 0) throw new Error("Package no longer exists");
  parcels[index] = {
    ...parcels[index],
    name: input.name.trim(),
    destination_country: input.destinationCountry?.trim() || null,
    destination_postcode: input.destinationPostcode?.trim() || null,
    shipment_date: input.shipmentDate?.trim() || null,
    international: input.international,
    updated_at: new Date().toISOString(),
  };
  writeMockParcels(parcels);
  return parcels[index];
}

async function setArchived(id: string, archived: boolean): Promise<Parcel> {
  if (isNative()) return invoke<Parcel>(archived ? "archive_parcel" : "restore_parcel", { id });
  const parcels = readMockParcels();
  const index = parcels.findIndex((item) => item.id === id);
  if (index < 0) throw new Error("Package no longer exists");
  parcels[index] = { ...parcels[index], archived_at: archived ? new Date().toISOString() : null };
  writeMockParcels(parcels);
  return parcels[index];
}

export const archiveParcel = (id: string) => setArchived(id, true);
export const restoreParcel = (id: string) => setArchived(id, false);

export async function deleteParcel(id: string): Promise<void> {
  if (isNative()) return invoke<void>("delete_parcel", { id });
  writeMockParcels(readMockParcels().filter((item) => item.id !== id));
}

export async function refreshParcel(id: string): Promise<void> {
  if (isNative()) return invoke<void>("refresh_parcel", { id });
  const parcels = readMockParcels();
  const index = parcels.findIndex((item) => item.id === id);
  if (index < 0) return;
  parcels[index] = { ...parcels[index], fetch_state: "fetching", last_error: null };
  writeMockParcels(parcels);
  await new Promise((resolve) => window.setTimeout(resolve, 450));
  const fresh = readMockParcels();
  const freshIndex = fresh.findIndex((item) => item.id === id);
  if (freshIndex >= 0) {
    const now = new Date().toISOString();
    fresh[freshIndex] = { ...fresh[freshIndex], fetch_state: "idle", last_checked_at: now, last_success_at: now };
    writeMockParcels(fresh);
  }
}

export async function refreshAll(): Promise<number> {
  if (isNative()) return invoke<number>("refresh_all");
  const active = readMockParcels().filter((parcel) => !parcel.archived_at && !["delivered", "returned", "cancelled"].includes(parcel.status));
  await Promise.all(active.map((parcel) => refreshParcel(parcel.id)));
  return active.length;
}

export async function getSettings(): Promise<Settings> {
  if (isNative()) return invoke<Settings>("get_settings");
  const saved = localStorage.getItem(MOCK_SETTINGS_KEY);
  return saved ? (JSON.parse(saved) as Settings) : structuredClone(mockSettings);
}

export async function updateSettings(settings: Settings): Promise<Settings> {
  if (isNative()) return invoke<Settings>("update_settings", { settings });
  localStorage.setItem(MOCK_SETTINGS_KEY, JSON.stringify(settings));
  return settings;
}

export async function getSourceHealth(): Promise<SourceHealth[]> {
  return isNative() ? invoke<SourceHealth[]>("get_source_health") : structuredClone(mockSourceHealth);
}

export async function openTrackingPage(id: string, partner = false): Promise<void> {
  if (isNative()) return invoke<void>("open_tracking_page", { id, partner });
  const parcel = mockParcel(id);
  const url = partner && parcel.international_tracking_url
    ? parcel.international_tracking_url
    : parcel.carrier === "dhl_paket_de"
    ? `https://www.dhl.de/de/privatkunden/dhl-sendungsverfolgung.html?piececode=${encodeURIComponent(parcel.tracking_number)}`
    : `https://www.myhermes.de/empfangen/sendungsverfolgung/sendungsinformation#${encodeURIComponent(parcel.tracking_number)}`;
  window.open(url, "_blank", "noopener,noreferrer");
}

export async function onParcelsChanged(callback: () => void): Promise<() => void> {
  if (!isNative()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  const unlistenUpdated = await listen("parcel_updated", callback);
  const unlistenDeleted = await listen("parcel_deleted", callback);
  return () => {
    unlistenUpdated();
    unlistenDeleted();
  };
}
