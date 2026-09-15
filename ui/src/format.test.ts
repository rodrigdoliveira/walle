import { describe, expect, it } from "vitest";

import { estimateText, fetchMessages, locationText, progressIndex, senderText, statusMessages } from "./format";
import type { ParcelStatus } from "./types";

describe("estimateText", () => {
  it("formats a delivery window", () => {
    expect(estimateText({ date: "2030-04-12", from: "12:00", until: "16:00" }))
      .toContain("12:00–16:00");
  });

  it("returns null when no estimate is available", () => {
    expect(estimateText(null)).toBeNull();
  });
});

describe("progressIndex", () => {
  it("maps the happy path to four progress steps", () => {
    const path: ParcelStatus[] = ["announced", "in_transit", "out_for_delivery", "delivered"];
    expect(path.map(progressIndex))
      .toEqual([0, 1, 2, 3]);
  });

  it("does not invent progress for an error state", () => {
    expect(progressIndex("exception")).toBe(-1);
  });
});

describe("English carrier presentation", () => {
  it("uses application-owned English status messages", () => {
    expect(statusMessages.announced).toBe("The carrier has received the shipment information.");
    expect(statusMessages.delivered).toBe("The shipment was delivered.");
    expect(fetchMessages.not_found).toBe("The carrier has not published tracking information for this number yet.");
  });

  it("translates generic German carrier metadata", () => {
    expect(senderText("Privatversand")).toBe("Private shipment");
    expect(locationText("Deutschland")).toBe("Germany");
    expect(locationText("Paketzentrum Berlin")).toBe("Carrier facility");
  });
});
