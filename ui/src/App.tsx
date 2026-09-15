import { useCallback, useEffect, useMemo, useState } from "react";
import { Archive, Box, Columns2, LayoutGrid, PackagePlus, RefreshCw, Search, Settings as SettingsIcon, SlidersHorizontal, Truck } from "lucide-react";
import {
  addParcel,
  archiveParcel,
  deleteParcel,
  getSettings,
  getSourceHealth,
  isNative,
  listParcels,
  onParcelsChanged,
  openTrackingPage,
  refreshAll,
  refreshParcel,
  restoreParcel,
  updateParcel,
  updateSettings,
} from "./api";
import { AddParcelDialog } from "./components/AddParcelDialog";
import { DetailsPanel } from "./components/DetailsPanel";
import { ParcelCard } from "./components/ParcelCard";
import { SettingsDialog } from "./components/SettingsDialog";
import { TitleBar } from "./components/TitleBar";
import { attentionStatuses, carrierLabels } from "./format";
import { mockSettings } from "./mock";
import type { AddParcelInput, Carrier, Parcel, Settings, SourceHealth, UpdateParcelInput } from "./types";

type View = "active" | "archive";
type Sort = "attention" | "expected" | "updated" | "added";
type PackageLayout = "comfortable" | "compact";

const PACKAGE_LAYOUT_KEY = "walle.packageLayout";
const demoMode = new URLSearchParams(window.location.search).get("demo");

function storedPackageLayout(): PackageLayout {
  try {
    return window.localStorage.getItem(PACKAGE_LAYOUT_KEY) === "compact" ? "compact" : "comfortable";
  } catch {
    return "comfortable";
  }
}

function messageOf(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

function attentionRank(parcel: Parcel): number {
  if (parcel.status === "ready_for_pickup") return 0;
  if (parcel.status === "out_for_delivery") return 1;
  if (attentionStatuses.has(parcel.status)) return 2;
  if (parcel.status === "unknown") return 4;
  return 3;
}

function expectedTime(parcel: Parcel): number {
  const value = parcel.estimate?.date ?? parcel.estimate?.from;
  if (!value) return Number.MAX_SAFE_INTEGER;
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? Number.MAX_SAFE_INTEGER : parsed;
}

export default function App() {
  const [parcels, setParcels] = useState<Parcel[]>([]);
  const [settings, setSettings] = useState<Settings>(mockSettings);
  const [sources, setSources] = useState<SourceHealth[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<View>("active");
  const [query, setQuery] = useState("");
  const [carrier, setCarrier] = useState<Carrier | "all">("all");
  const [sort, setSort] = useState<Sort>("attention");
  const [packageLayout, setPackageLayout] = useState<PackageLayout>(() => demoMode === "compact" ? "compact" : storedPackageLayout());
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [refreshingAll, setRefreshingAll] = useState(false);
  const [announcement, setAnnouncement] = useState("");

  const load = useCallback(async (quiet = false) => {
    if (!quiet) setLoading(true);
    try {
      const [nextParcels, nextSettings, nextSources] = await Promise.all([listParcels(), getSettings(), getSourceHealth()]);
      setParcels(nextParcels);
      setSettings(nextSettings);
      setSources(nextSources);
      setError(null);
    } catch (reason) {
      setError(messageOf(reason));
    } finally {
      if (!quiet) setLoading(false);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    if (loading || isNative()) return;
    if (demoMode === "details" && !selectedId) setSelectedId(parcels.find((parcel) => parcel.id === "demo-pickup")?.id ?? null);
    if (demoMode === "settings" && !settingsOpen) setSettingsOpen(true);
  }, [loading, parcels, selectedId, settingsOpen]);
  useEffect(() => {
    let dispose: () => void = () => undefined;
    void onParcelsChanged(() => void load(true)).then((unlisten) => { dispose = unlisten; });
    return () => dispose();
  }, [load]);
  useEffect(() => {
    if (settings.theme === "system") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = settings.theme;
  }, [settings.theme]);
  useEffect(() => {
    try { window.localStorage.setItem(PACKAGE_LAYOUT_KEY, packageLayout); } catch { /* Keep the session choice if storage is unavailable. */ }
  }, [packageLayout]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key.toLowerCase() === "n") { event.preventDefault(); setAddOpen(true); }
      if (event.key === "Escape" && selectedId) setSelectedId(null);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selectedId]);

  const counts = useMemo(() => ({
    active: parcels.filter((parcel) => !parcel.archived_at).length,
    delivered: parcels.filter((parcel) => !parcel.archived_at && parcel.status === "delivered").length,
    archive: parcels.filter((parcel) => parcel.archived_at).length,
  }), [parcels]);

  const visible = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    const items = parcels.filter((parcel) => {
      const inView = view === "archive"
        ? Boolean(parcel.archived_at)
        : !parcel.archived_at;
      return inView
        && (carrier === "all" || parcel.carrier === carrier)
        && (!needle || parcel.name.toLocaleLowerCase().includes(needle) || parcel.tracking_number.toLocaleLowerCase().includes(needle));
    });
    return items.sort((left, right) => {
      if (sort === "attention") return attentionRank(left) - attentionRank(right) || Date.parse(right.created_at) - Date.parse(left.created_at) || left.id.localeCompare(right.id);
      if (sort === "expected") return expectedTime(left) - expectedTime(right) || left.id.localeCompare(right.id);
      if (sort === "updated") return Date.parse(right.events[0]?.timestamp ?? right.updated_at) - Date.parse(left.events[0]?.timestamp ?? left.updated_at) || left.id.localeCompare(right.id);
      return Date.parse(right.created_at) - Date.parse(left.created_at) || left.id.localeCompare(right.id);
    });
  }, [parcels, query, carrier, sort, view]);

  const selected = parcels.find((parcel) => parcel.id === selectedId) ?? null;
  const viewHeading = view === "archive" ? "Archive bay" : "Package bay";
  const viewCopy = view === "archive"
    ? "Stored shipments and their local tracking history."
    : "Live carrier updates, stored locally on this computer.";

  const replaceParcel = (parcel: Parcel) => setParcels((current) => current.map((item) => item.id === parcel.id ? parcel : item));
  const handleError = (reason: unknown) => { const message = messageOf(reason); setAnnouncement(message); setError(message); };

  const create = async (input: AddParcelInput) => {
    const parcel = await addParcel(input);
    setParcels((current) => [parcel, ...current.filter((item) => item.id !== parcel.id)]);
    setSelectedId(parcel.id);
    setAnnouncement(`${parcel.name} was added and queued for its first update.`);
  };

  const update = async (input: UpdateParcelInput) => {
    try { const parcel = await updateParcel(input); replaceParcel(parcel); setAnnouncement(`${parcel.name} was updated.`); }
    catch (reason) { handleError(reason); throw reason; }
  };

  const archive = async (id: string) => {
    try { const parcel = await archiveParcel(id); replaceParcel(parcel); setAnnouncement(`${parcel.name} was archived.`); }
    catch (reason) { handleError(reason); }
  };
  const restore = async (id: string) => {
    try { const parcel = await restoreParcel(id); replaceParcel(parcel); setAnnouncement(`${parcel.name} was restored.`); }
    catch (reason) { handleError(reason); }
  };
  const remove = async (id: string) => {
    try { await deleteParcel(id); setParcels((current) => current.filter((item) => item.id !== id)); setAnnouncement("Package deleted from this computer."); }
    catch (reason) { handleError(reason); }
  };
  const refresh = async (id: string) => {
    try {
      setParcels((current) => current.map((item) => item.id === id ? { ...item, fetch_state: "queued" } : item));
      await refreshParcel(id);
      await load(true);
      setAnnouncement("Package refresh completed.");
    } catch (reason) { handleError(reason); }
  };
  const refreshEverything = async () => {
    setRefreshingAll(true);
    try {
      const count = await refreshAll();
      setAnnouncement(`${count} active ${count === 1 ? "package" : "packages"} queued for refresh.`);
      await load(true);
    } catch (reason) { handleError(reason); }
    finally { setRefreshingAll(false); }
  };
  const saveSettings = async (next: Settings) => {
    try { const saved = await updateSettings(next); setSettings(saved); setAnnouncement("Settings saved."); }
    catch (reason) { handleError(reason); throw reason; }
  };

  return (
    <>
      <TitleBar />
      <div className="app-shell">
      <aside className="app-sidebar">
        <div className="brand">
          <img className="brand-optics" src="/walle-icon.svg" alt="" aria-hidden="true" />
          <div><strong>WALLE</strong><span>Parcel control</span></div>
        </div>
        <span className="sidebar-label">Tracking bay</span>
        <nav className="side-nav" aria-label="Package views">
          {([
            ["active", "Packages", counts.active, Truck],
            ["archive", "Archive", counts.archive, Archive],
          ] as const).map(([value, label, count, Icon]) => (
            <button key={value} aria-current={view === value ? "page" : undefined} onClick={() => setView(value)}><Icon size={18} /><span>{label}</span><b>{count}</b></button>
          ))}
        </nav>
        <div className="sidebar-spacer" />
        <button className="side-settings" onClick={() => setSettingsOpen(true)}><SettingsIcon size={18} /><span>Settings</span></button>
        <div className="vent" aria-hidden="true"><i /><i /><i /><i /></div>
      </aside>

      <section className="workspace">
        <header className="app-header">
          <div className="header-copy"><span className="eyebrow">Local tracking console</span><h1>{viewHeading}</h1><p>{viewCopy}</p></div>
          <div className="header-actions">
            <button className="button-secondary hide-compact" onClick={() => void refreshEverything()} disabled={refreshingAll}><RefreshCw size={17} className={refreshingAll ? "spinning" : ""} /> Refresh all</button>
            <button className="button-primary" onClick={() => setAddOpen(true)}><PackagePlus size={17} /> Add package</button>
          </div>
        </header>

        <main className="app-main">
          <section className="toolbar-panel">
            <span className="panel-screw screw-nw" /><span className="panel-screw screw-ne" />
            <div className="toolbar" aria-label="Package filters">
              <label className="search-box"><Search size={18} /><span className="sr-only">Search packages</span><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search name or tracking number" /></label>
              <label className="compact-select"><SlidersHorizontal size={16} /><span className="sr-only">Carrier</span><select value={carrier} onChange={(event) => setCarrier(event.target.value as Carrier | "all")}><option value="all">All carriers</option>{Object.entries(carrierLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
              <label className="compact-select"><span className="sr-only">Sort packages</span><select value={sort} onChange={(event) => setSort(event.target.value as Sort)}><option value="attention">Needs attention</option><option value="expected">Expected delivery</option><option value="updated">Recently updated</option><option value="added">Recently added</option></select></label>
              <div className="layout-switch" role="group" aria-label="Package layout">
                <button type="button" className={packageLayout === "comfortable" ? "active" : ""} aria-pressed={packageLayout === "comfortable"} title="Comfortable two-column layout" onClick={() => setPackageLayout("comfortable")}><Columns2 size={17} /><span className="sr-only">Two-column layout</span></button>
                <button type="button" className={packageLayout === "compact" ? "active" : ""} aria-pressed={packageLayout === "compact"} title="Compact four-column layout" onClick={() => setPackageLayout("compact")}><LayoutGrid size={17} /><span className="sr-only">Four-column compact layout</span></button>
              </div>
            </div>
          </section>

        {error && <div className="page-error" role="alert"><strong>Walle hit a problem</strong><span>{error}</span><button onClick={() => setError(null)}>Dismiss</button></div>}

        {loading ? (
          <div className="loading-grid" aria-label="Loading packages">{[1, 2, 3, 4].map((item) => <span key={item} />)}</div>
        ) : visible.length ? (
          <section className={`parcel-grid layout-${packageLayout}`} aria-label={`${view} packages`}>
            {visible.map((parcel) => <ParcelCard key={parcel.id} parcel={parcel} onOpen={setSelectedId} onRefresh={(id) => void refresh(id)} onArchive={(id) => void archive(id)} onRestore={(id) => void restore(id)} />)}
            {view === "active" && visible.length % (packageLayout === "compact" ? 4 : 2) !== 0 && (
              <button className="empty-bay" onClick={() => setAddOpen(true)}><Box size={45} /><strong>Another package soon</strong><span>Add package</span></button>
            )}
          </section>
        ) : (
          <section className="empty-state">
            <span><Archive size={27} /></span>
            <h2>{parcels.length ? "No packages match these filters" : "Your deliveries will appear here"}</h2>
            <p>{parcels.length ? "Try another view, carrier, or search." : "Add a tracking number from DHL or Hermes to get started."}</p>
            {parcels.length ? <button className="button-secondary" onClick={() => { setQuery(""); setCarrier("all"); }}>Reset filters</button> : <button className="button-primary" onClick={() => setAddOpen(true)}><PackagePlus size={17} /> Add your first package</button>}
          </section>
        )}
        </main>
      </section>

      <AddParcelDialog open={addOpen} onClose={() => setAddOpen(false)} onSubmit={create} />
      <SettingsDialog open={settingsOpen} settings={settings} sources={sources} onClose={() => setSettingsOpen(false)} onSave={saveSettings} />
      <DetailsPanel parcel={selected} onClose={() => setSelectedId(null)} onRefresh={refresh} onArchive={archive} onRestore={restore} onDelete={remove} onUpdate={update} onOpenCarrier={openTrackingPage} />
      <div className="sr-only" aria-live="polite">{announcement}</div>
      </div>
    </>
  );
}
