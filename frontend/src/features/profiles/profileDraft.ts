export type DraftSync = {
  draft: string;
  lastSynced: string;
  externalConflict: boolean;
};

export function reconcileExternalText(
  current: DraftSync,
  diskText: string,
): DraftSync {
  if (diskText === current.lastSynced || diskText === current.draft) {
    return current;
  }
  if (current.draft === current.lastSynced) {
    return {
      draft: diskText,
      lastSynced: diskText,
      externalConflict: false,
    };
  }
  return { ...current, externalConflict: true };
}

export function finishSave(
  currentDraft: string,
  savedText: string,
): Pick<DraftSync, "lastSynced"> & { saveState: "saved" | "dirty" } {
  return {
    lastSynced: savedText,
    saveState: currentDraft === savedText ? "saved" : "dirty",
  };
}
