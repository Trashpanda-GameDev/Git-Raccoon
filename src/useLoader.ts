import { useEffect, useRef, useState } from "react";

/**
 * Loads data for `key`, dropping responses that arrive after the key
 * changed. While a new key loads, the previous data stays available.
 */
export function useLoader<T>(key: string | null, load: (key: string) => Promise<T>) {
  const [state, setState] = useState<{ key: string | null; data: T | null; error: string; loading: boolean }>({
    key: null,
    data: null,
    error: "",
    loading: false,
  });
  const latest = useRef<string | null>(null);

  useEffect(() => {
    latest.current = key;
    if (key === null) {
      setState({ key: null, data: null, error: "", loading: false });
      return;
    }
    setState((s) => ({ ...s, key, loading: true, error: "" }));
    load(key).then(
      (data) => latest.current === key && setState({ key, data, error: "", loading: false }),
      (e) => latest.current === key && setState({ key, data: null, error: String(e), loading: false }),
    );
    // `load` is a fresh closure each render; `key` captures everything it reads.
  }, [key]);

  return state;
}
