import { describe, expect, it } from "vitest";

import { finishSave, reconcileExternalText } from "./profileDraft";

describe("profile draft synchronization", () => {
  it("adopts an external edit when the local draft is clean", () => {
    expect(
      reconcileExternalText(
        { draft: "old", lastSynced: "old", externalConflict: false },
        "disk",
      ),
    ).toEqual({
      draft: "disk",
      lastSynced: "disk",
      externalConflict: false,
    });
  });

  it("preserves a dirty draft and reports an external conflict", () => {
    expect(
      reconcileExternalText(
        { draft: "local", lastSynced: "old", externalConflict: false },
        "disk",
      ),
    ).toEqual({
      draft: "local",
      lastSynced: "old",
      externalConflict: true,
    });
  });

  it("keeps a newer edit dirty when an older autosave finishes", () => {
    expect(finishSave("newer edit", "first edit")).toEqual({
      lastSynced: "first edit",
      saveState: "dirty",
    });
    expect(finishSave("first edit", "first edit")).toEqual({
      lastSynced: "first edit",
      saveState: "saved",
    });
  });
});
