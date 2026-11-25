import { useCallback, useEffect, useRef, useState } from 'react';
import { PhoneNwcSession } from './nwc';
import type { BrowserProvider } from './webrgb';

export function usePhoneWallet() {
  const [provider, setProvider] = useState<BrowserProvider>();
  const [uri, setUri] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const session = useRef<PhoneNwcSession | undefined>(undefined);

  const disconnect = useCallback(() => {
    const current = session.current;
    session.current = undefined;
    current?.close();
    setProvider(undefined);
    setUri(undefined);
    setBusy(false);
    setError(undefined);
  }, []);

  const connect = useCallback(async () => {
    if (session.current) return;
    const current = new PhoneNwcSession(undefined, reason => {
      if (session.current !== current) return;
      disconnect();
      setError(reason);
    });
    session.current = current;
    setError(undefined);
    setBusy(true);
    try {
      const wallet = await current.open(setUri);
      if (session.current === current) {
        setProvider(wallet);
        setUri(undefined);
        setBusy(false);
      }
    } catch { /* The session publishes its closing reason. */ }
  }, [disconnect]);
  useEffect(() => disconnect, [disconnect]);
  return { provider, busy, error, uri, connect, disconnect };
}
