import type { Snapshot, StorageVolumeStatus } from "../api";

/** Check if a snapshot is a database dump based on tags + storage info. */
export function isSnapshotDbDump(
  snap: Snapshot,
  appId: string,
  storage?: StorageVolumeStatus[],
): boolean {
  if (!storage || storage.length === 0) return false;
  for (const tag of snap.tags) {
    const prefix = appId + "/";
    if (tag.startsWith(prefix)) {
      const volName = tag.slice(prefix.length);
      const vol = storage.find((s) => s.name === volName);
      if (vol?.is_db_dump) return true;
    }
  }
  return false;
}

/** Resolve a snapshot's tags to the original host path using app storage info. */
export function resolveRestorePath(
  snap: Snapshot,
  appId: string,
  storage?: StorageVolumeStatus[],
): string | undefined {
  if (!storage || storage.length === 0) return undefined;
  for (const tag of snap.tags) {
    const prefix = appId + "/";
    if (tag.startsWith(prefix)) {
      const volName = tag.slice(prefix.length);
      const vol = storage.find((s) => s.name === volName);
      if (vol) return vol.host_path;
    }
  }
  return undefined;
}
