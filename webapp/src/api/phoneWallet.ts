import { useCallback, useEffect, useRef, useState } from 'react';
import type { WalletConnectConnection, WalletConnectProvider } from '@utexo/webrgb-walletconnect';
import type { BrowserProvider } from './webrgb';

const NETWORK = 'mutinynet';
const projectId = import.meta.env.VITE_WALLETCONNECT_PROJECT_ID?.trim();
const TOPIC_KEY = 'mosaik.phone.mutinynet.topic';
export const phoneWalletConfigured = Boolean(projectId);

async function initClient() {
  const { SignClient } = await import('@walletconnect/sign-client');
  return SignClient.init({ projectId, telemetryEnabled: false, metadata: {
    name: 'Mosaik', description: 'RGB marketplace on Mutinynet',
    url: window.location.origin, icons: [],
  } });
}
let client: ReturnType<typeof initClient> | undefined;
function getClient() {
  if (!client) client = initClient().catch(error => { client = undefined; throw error; });
  return client;
}

function remember(topic?: string) {
  try { if (topic) sessionStorage.setItem(TOPIC_KEY, topic); else sessionStorage.removeItem(TOPIC_KEY); } catch { /* storage disabled */ }
}
function remembered() {
  try { return sessionStorage.getItem(TOPIC_KEY); } catch { return null; }
}

async function startPhonePairing() {
  const { connectWalletConnect } = await import('@utexo/webrgb-walletconnect');
  return connectWalletConnect({
    client: await getClient(), network: NETWORK, methods: ['getBtcBalance', 'listAssets'],
    optionalMethods: ['createUtxos', 'refresh'],
  });
}
async function restore(topic: string) {
  const { createWalletConnectProvider } = await import('@utexo/webrgb-walletconnect');
  return createWalletConnectProvider({ client: await getClient(), network: NETWORK, topic });
}

async function walletAdapter(remote: WalletConnectProvider): Promise<BrowserProvider> {
  const { toAssetArray } = await import('@utexo/webrgb');
  await remote.enable();
  const info = await remote.getInfo();
  if (!info.ready || info.network !== NETWORK) throw new Error('Unlock your RGB wallet on Mutinynet, then reconnect.');
  const provider: BrowserProvider = {
    getBtcBalance: () => remote.getBtcBalance(),
    listAssets: async () => toAssetArray(await remote.listAssets()),
    getInfo: () => remote.getInfo(),
  };
  if (info.methods.includes('createUtxos')) provider.createUtxos = args => remote.createUtxos(args);
  if (info.methods.includes('refresh')) provider.refresh = () => remote.refresh();
  return provider;
}

export function usePhoneWallet() {
  const [provider, setProvider] = useState<BrowserProvider>();
  const [uri, setUri] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const generation = useRef(0);
  const connecting = useRef(false);
  const pairing = useRef<WalletConnectConnection>(undefined);
  const session = useRef<WalletConnectProvider>(undefined);
  const unsubscribe = useRef<(() => void) | undefined>(undefined);

  const reset = useCallback((forget: boolean) => {
    generation.current++;
    connecting.current = false;
    unsubscribe.current?.();
    unsubscribe.current = undefined;
    const remote = session.current;
    const pending = pairing.current;
    session.current = undefined;
    pairing.current = undefined;
    if (forget) {
      remember();
      if (pending) void pending.cancel().catch(() => {});
      else if (remote) void remote.disconnect().catch(() => {});
    } else {
      if (pending) void pending.cancel().catch(() => {});
      remote?.dispose();
    }
    setProvider(undefined); setUri(undefined); setBusy(false); setError(undefined);
  }, []);

  const connect = useCallback(async (topic?: string) => {
    if (!phoneWalletConfigured || connecting.current || session.current) return;
    const id = ++generation.current;
    connecting.current = true;
    setError(undefined);
    setBusy(true);
    try {
      let remote: WalletConnectProvider;
      if (topic) {
        try { remote = await restore(topic); }
        catch (e) { if (id === generation.current) remember(); throw e; }
      } else {
        const connection = await startPhonePairing();
        if (id !== generation.current) { await connection.cancel(); return; }
        pairing.current = connection;
        setUri(connection.uri);
        remote = await connection.approval();
      }
      if (id !== generation.current) { remote.dispose(); return; }
      session.current = remote;
      setUri(undefined);
      const adapter = await walletAdapter(remote);
      const transport = await getClient();
      if (id !== generation.current) return;
      remember(remote.topic);
      pairing.current = undefined;
      const lost = (event: { topic: string }) => {
        if (event.topic !== remote.topic || id !== generation.current) return;
        reset(false); remember(); setError('The phone wallet disconnected.');
      };
      transport.on('session_delete', lost);
      transport.on('session_expire', lost);
      const stopAvailability = remote.subscribeAvailability(state => {
        if (id !== generation.current) return;
        if (state.authorization === 'REVOKED') {
          reset(true);
          setError('The phone wallet disconnected.');
        } else if (!state.enabled || state.info?.ready === false) {
          setProvider(undefined);
          setBusy(true);
          setError('Waiting for your phone wallet to reconnect.');
        } else if (state.info?.ready) {
          setProvider(adapter);
          setBusy(false);
          setError(undefined);
        }
      });
      unsubscribe.current = () => {
        stopAvailability();
        transport.off('session_delete', lost);
        transport.off('session_expire', lost);
      };
      setProvider(adapter);
    } catch (e) {
      if (id !== generation.current) return;
      reset(true);
      setError(e instanceof Error ? e.message : 'Phone wallet connection failed.');
    } finally {
      if (id === generation.current) { connecting.current = false; setBusy(false); }
    }
  }, [reset]);

  useEffect(() => {
    const topic = remembered();
    if (topic) void connect(topic);
    return () => reset(false);
  }, [connect, reset]);
  return { provider, busy, error, uri, connect, disconnect: () => reset(true) };
}
