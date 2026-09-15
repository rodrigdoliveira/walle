import dhlLogo from "../assets/carriers/dhl.svg";
import hermesLogo from "../assets/carriers/hermes.svg";
import { carrierLabels } from "../format";
import type { Carrier } from "../types";

const carrierLogos: Record<Carrier, string> = {
  dhl_paket_de: dhlLogo,
  hermes_de: hermesLogo,
};

interface Props {
  carrier: Carrier;
}

export function CarrierLogo({ carrier }: Props) {
  return (
    <span className={`carrier-logo carrier-${carrier}`}>
      <img src={carrierLogos[carrier]} alt={`${carrierLabels[carrier]} logo`} />
    </span>
  );
}
