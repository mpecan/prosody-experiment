// What the page remembers per browser. Storage can be missing or refused (a private window, blocked
// site data), so every read and write is allowed to fail: the page then starts from its defaults.

export function load(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function save(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembered; nothing else changes.
  }
}

/** A JSON value saved under `key`, or null if there is none or it cannot be read. */
export function loadJson(key: string): unknown {
  try {
    return JSON.parse(load(key) ?? 'null');
  } catch {
    return null;
  }
}
