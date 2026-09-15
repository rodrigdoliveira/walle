import { useEffect, useRef, useState } from "react";
import { ChevronDown, PackagePlus, X } from "lucide-react";
import type { AddParcelInput, Carrier } from "../types";
import { carrierLabels } from "../format";

interface Props {
  open: boolean;
  onClose: () => void;
  onSubmit: (input: AddParcelInput) => Promise<void>;
}

function suggestedCarrier(value: string): Carrier | null {
  const number = value.trim().toUpperCase();
  if (/^\d{14}$/.test(number) || /^H\d{19}$/.test(number)) return "hermes_de";
  if (number.endsWith("DE") || number.length >= 16) return "dhl_paket_de";
  return null;
}

export function AddParcelDialog({ open, onClose, onSubmit }: Props) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState("");
  const [trackingNumber, setTrackingNumber] = useState("");
  const [carrier, setCarrier] = useState<Carrier>("dhl_paket_de");
  const [carrierTouched, setCarrierTouched] = useState(false);
  const [showOptional, setShowOptional] = useState(false);
  const [destinationCountry, setDestinationCountry] = useState("");
  const [destinationPostcode, setDestinationPostcode] = useState("");
  const [shipmentDate, setShipmentDate] = useState("");
  const [international, setInternational] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  useEffect(() => {
    if (carrierTouched) return;
    const suggestion = suggestedCarrier(trackingNumber);
    if (suggestion) setCarrier(suggestion);
  }, [trackingNumber, carrierTouched]);

  const reset = () => {
    setName("");
    setTrackingNumber("");
    setCarrier("dhl_paket_de");
    setCarrierTouched(false);
    setShowOptional(false);
    setDestinationCountry("");
    setDestinationPostcode("");
    setShipmentDate("");
    setInternational(false);
    setError(null);
  };

  const close = () => {
    if (submitting) return;
    reset();
    onClose();
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError(null);
    if (!name.trim() || !trackingNumber.trim()) {
      setError("Name and tracking number are required.");
      return;
    }
    setSubmitting(true);
    try {
      await onSubmit({
        name,
        trackingNumber,
        carrier,
        destinationCountry: destinationCountry || null,
        destinationPostcode: destinationPostcode || null,
        shipmentDate: shipmentDate || null,
        international,
      });
      close();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <dialog ref={dialogRef} className="modal" onCancel={(event) => { event.preventDefault(); close(); }} onClose={onClose}>
      <form onSubmit={submit}>
        <header className="modal-header">
          <div><span className="eyebrow">New package</span><h2>Add tracking</h2></div>
          <button type="button" className="icon-button" onClick={close} aria-label="Close add package"><X size={19} /></button>
        </header>
        <div className="modal-body form-stack">
          <label>Package name<input autoFocus maxLength={120} value={name} onChange={(event) => setName(event.target.value)} placeholder="e.g. Studio headphones" /></label>
          <label>Tracking number<input maxLength={100} spellCheck={false} value={trackingNumber} onChange={(event) => setTrackingNumber(event.target.value)} placeholder="Paste the carrier number" /></label>
          <label>Carrier
            <select value={carrier} onChange={(event) => { setCarrierTouched(true); setCarrier(event.target.value as Carrier); }}>
              {Object.entries(carrierLabels).map(([value, label]) => <option value={value} key={value}>{label}</option>)}
            </select>
          </label>
          <label>Recipient postcode <span className="label-optional">Optional</span>
            <input value={destinationPostcode} maxLength={16} onChange={(event) => setDestinationPostcode(event.target.value)} placeholder={carrier === "dhl_paket_de" ? "e.g. 10115" : "Only needed for some recipient services"} />
            <small className="field-hint">
              {carrier === "dhl_paket_de"
                ? "DHL may request this to verify the recipient and provide live tracking or detailed delivery information."
                : "Hermes status tracking uses the parcel number alone. Some recipient services on the Hermes website may request a postcode."}
            </small>
          </label>
          <button type="button" className="disclosure" onClick={() => setShowOptional((value) => !value)} aria-expanded={showOptional}>
            <ChevronDown size={17} className={showOptional ? "rotated" : ""} /> Optional carrier details
          </button>
          {showOptional && (
            <div className="optional-fields">
              <label>Destination country<input value={destinationCountry} maxLength={2} onChange={(event) => setDestinationCountry(event.target.value.toUpperCase())} placeholder="DE" /></label>
              {carrier === "dhl_paket_de" && <label>Shipment date<input type="date" value={shipmentDate} onChange={(event) => setShipmentDate(event.target.value)} /></label>}
              <label className="check-row"><input type="checkbox" checked={international} onChange={(event) => setInternational(event.target.checked)} /> International shipment</label>
            </div>
          )}
          {error && <p className="form-error" role="alert">{error}</p>}
        </div>
        <footer className="modal-footer">
          <button type="button" className="button-secondary" onClick={close}>Cancel</button>
          <button type="submit" className="button-primary" disabled={submitting}><PackagePlus size={17} /> {submitting ? "Adding…" : "Add package"}</button>
        </footer>
      </form>
    </dialog>
  );
}
