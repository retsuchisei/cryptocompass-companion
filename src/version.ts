/**
 * One version, declared in three manifests.
 *
 * A release is a tag, so nothing at build time forces `package.json`,
 * `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` to name the same
 * version - and each of the three is read by something different: the
 * installer's name, the updater's manifest, and `CARGO_PKG_VERSION`, which is
 * what consent is stored against. The rule lives here because the gate's only
 * JavaScript is the interface's; nothing in the interface imports it.
 */

export type Declared = { where: string; version: string };

/**
 * The declarations that do not match the first one, by name. The first is the
 * one the others are read against rather than a majority, so the answer is the
 * same however many manifests are compared.
 */
export function disagreements(declared: Declared[]): string[] {
  const [first, ...rest] = declared;
  if (!first) {
    return [];
  }

  return rest.filter((one) => one.version !== first.version).map((one) => one.where);
}

/**
 * The crate's own version out of a Cargo manifest - the `version` under
 * `[package]`, not a dependency's, which is the same key one table further
 * down.
 */
export function versionInCargoToml(text: string): string | null {
  let inPackage = false;

  for (const line of text.split("\n")) {
    const trimmed = line.trim();

    if (trimmed.startsWith("[")) {
      inPackage = trimmed === "[package]";
      continue;
    }

    const version = inPackage ? /^version\s*=\s*"([^"]+)"/.exec(trimmed) : null;
    if (version) {
      return version[1];
    }
  }

  return null;
}
