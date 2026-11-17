// Remote mode: this app controlling a MetroQueue phone, TV or another desktop (Rust
// `metroqueue/remote.rs`). While `remote.state` is set the player bar shows that device's player
// and its controls go there as `/api/cmd` (PROTOCOL.md §5); the local player stays paused.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { t } from './i18n.svelte';

export interface RemoteTrack {
	id: string;
	title: string;
	artist: string;
	art: string | null;
	thumb: string | null;
	duration: number;
	artists: { id: string | null; name: string }[];
	episode: boolean;
}

export interface RemoteState {
	track: RemoteTrack | null;
	playing: boolean;
	position: number;
	duration: number;
	shuffle: boolean;
	repeat: number;
	volume: number;
	index: number;
	queueHash: number;
	playbackState: number;
	speed: number;
}

interface RemoteView {
	id: string;
	name: string;
	kind: string;
	status: 'connected' | 'reconnecting';
	state: RemoteState;
	queue: RemoteTrack[];
}

export interface Receiver {
	id: string;
	name: string;
	kind: string;
	online: boolean;
	paired: boolean;
}

export const remote = $state({
	device: null as { id: string; name: string; kind: string } | null,
	status: null as RemoteView['status'] | null,
	state: null as RemoteState | null,
	queue: [] as RemoteTrack[],
	/** Seconds, extrapolated between pushes like the phone does. */
	position: 0,
	/** Restored by unmute. */
	lastVolume: 100
});

let receivedAt = 0;
let ticker: ReturnType<typeof setInterval> | undefined;

function extrapolate() {
	const s = remote.state;
	if (!s) return;
	const moving = s.playing && s.playbackState === 3;
	const ms = s.position + (moving ? (Date.now() - receivedAt) * s.speed : 0);
	remote.position = Math.min(ms, s.duration || ms) / 1000;
}

function apply(v: RemoteView | null) {
	remote.device = v && { id: v.id, name: v.name, kind: v.kind };
	remote.status = v?.status ?? null;
	remote.state = v?.state ?? null;
	remote.queue = v?.queue ?? [];
	if (v && v.state.volume > 0) remote.lastVolume = Math.round(v.state.volume * 100);
	receivedAt = Date.now();
	extrapolate();
	// Ticks only while something there is playing.
	const moving = !!v?.state.playing;
	if (moving && !ticker) ticker = setInterval(extrapolate, 250);
	else if (!moving && ticker) {
		clearInterval(ticker);
		ticker = undefined;
	}
}

/** Wire the Rust events; returns the teardown. */
export function initRemote(onEnded: (reason: string) => void): () => void {
	invoke<RemoteView | null>('mq_remote_status').then(apply).catch(() => {});
	const uns = [
		listen<RemoteView | null>('mq://remote', (e) => apply(e.payload)),
		listen<string>('mq://remote-ended', (e) => onEnded(e.payload))
	];
	return () => {
		uns.forEach((un) => un.then((f) => f()));
		apply(null);
	};
}

export function remoteCmd(action: string, a = 0, b = 0): Promise<void> {
	return invoke<void>('mq_remote_cmd', { action, a: Math.round(a), b: Math.round(b) });
}

const ERRORS = {
	unreachable: 'devices.errors.unreachable',
	not_receiver: 'devices.errors.not_receiver',
	wrong_code: 'devices.errors.wrong_code',
	expired: 'devices.errors.expired',
	locked: 'devices.errors.locked',
	not_paired: 'devices.errors.not_paired',
	busy: 'devices.errors.busy',
	lost: 'devices.errors.lost'
} as const;

/** Words an error code from `remote.rs`. */
export function remoteError(code: string): string {
	return t(ERRORS[code as keyof typeof ERRORS] ?? 'devices.errors.failed');
}
