import { invoke } from "@tauri-apps/api/core";

export type PackManifest = {
  schema_version: number;
  format: string;
  character_id: string;
  files: string[];
};

export type ImportResult = {
  character_id: string;
  files_restored: number;
  backup_path: string | null;
};

export const portabilityClient = {
  exportPack(vaultRoot: string, destination: string) {
    return invoke<PackManifest>("vault_export_pack", { vaultRoot, destination });
  },
  importPack(packRoot: string, destinationVault: string, replaceExisting = false) {
    return invoke<ImportResult>("vault_import_pack", {
      packRoot,
      destinationVault,
      replaceExisting,
    });
  },
};
