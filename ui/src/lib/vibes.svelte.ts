// Vibe mode (MetroQueue's `ui/vibe/Vibe.kt`, `CustomVibes.kt`): the presets, the custom vibes
// synced with the phone (stored by Rust, `mqsync.rs`), the vibe on screen and its music.
import { invoke } from '@tauri-apps/api/core';
import * as api from '$lib/api';
import { t } from '$lib/i18n.svelte';
import { toast } from '$lib/player.svelte';
import { VIBE_SCENES, readScene, type SceneSpec } from '$lib/vibescene';

/** First set source wins: a local playlist with songs, a YouTube playlist, else a radio from `seed`. */
export interface VibeMusic {
	localPlaylistId?: string | null;
	/** A YouTube playlist id, without the `VL` of a browse id. */
	playlistId?: string | null;
	seed?: string | null;
	playlistTitle?: string | null;
	playlistThumbnail?: string | null;
}

export interface Vibe {
	id: string;
	name: string;
	scene: SceneSpec;
	music: VibeMusic;
	/** The JSON as synced. Saved back with only the edited fields changed, since a newer phone may
	 *  have put fields there this app doesn't know (SYNC.md). */
	raw: Record<string, unknown>;
}

const ID_PREFIX = 'custom_';
export const isCustom = (id: string) => id.startsWith(ID_PREFIX);
export const newVibeId = () => ID_PREFIX + crypto.randomUUID();

const preset = (id: string, name: string, scene: SceneSpec, seed: string): Vibe => ({
	id,
	name,
	scene,
	music: { seed },
	raw: {}
});

/** The presets (named in the current language) followed by the user's own vibes. */
export const allVibes = (): Vibe[] => [
	preset('winter_dusk', t('vibes.winter_dusk'), VIBE_SCENES.winterDusk, 'lofi winter chill'),
	preset('rainy_city', t('vibes.rainy_city'), VIBE_SCENES.rainyCity, 'lofi rainy night city'),
	preset('starry_lake', t('vibes.starry_lake'), VIBE_SCENES.starryLake, 'ambient night sky chill'),
	preset('spring_blossoms', t('vibes.spring_blossoms'), VIBE_SCENES.springBlossoms, 'japanese lofi spring'),
	...vibes.custom
];

export const vibes = $state({
	custom: [] as Vibe[],
	/** The vibe in the full-screen player, if one is open. */
	playing: null as Vibe | null,
	/** The window is on screen (not hidden to the tray); scenes stop animating otherwise. */
	uiVisible: true
});

function fromJson(v: unknown): Vibe | null {
	if (!v || typeof v !== 'object') return null;
	const raw = v as Record<string, unknown>;
	if (typeof raw.id !== 'string' || !isCustom(raw.id)) return null;
	const music = raw.music && typeof raw.music === 'object' ? (raw.music as VibeMusic) : {};
	return { id: raw.id, name: String(raw.name ?? ''), scene: readScene(raw.scene), music, raw };
}

export async function loadVibes(): Promise<void> {
	try {
		const list = await invoke<unknown[]>('mq_vibes');
		vibes.custom = list.map(fromJson).filter((v) => v !== null);
	} catch (e) {
		toast.error(String(e));
	}
}

/** Saves a custom vibe; `music` replaces the stored one only when given. Rust stamps `updatedAt`. */
export async function saveVibe(base: Vibe, name: string, scene: SceneSpec, music?: VibeMusic) {
	const raw = base.raw;
	const vibe = {
		...raw,
		id: base.id,
		name,
		scene: { ...(raw.scene as object), ...scene },
		music: music ? { ...(raw.music as object), ...music } : (raw.music ?? base.music)
	};
	await invoke('mq_save_vibe', { vibe });
	await loadVibes();
}

export async function deleteVibe(id: string) {
	await invoke('mq_delete_vibe', { id });
	await loadVibes();
}

/** Opens the full-screen player, starting the vibe's music first when asked. */
export function openVibe(vibe: Vibe, playMusic: boolean) {
	vibes.playing = vibe;
	if (playMusic) playVibeMusic(vibe.music);
}

async function playVibeMusic(music: VibeMusic) {
	try {
		const local = music.localPlaylistId;
		// A phone's library id means nothing here; it then plays the YouTube playlist below.
		if (local && (api.isLocalPlaylist(local) || local === api.ON_REPEAT_ID)) {
			const page = await api.getPlaylist(local);
			if (page.items.length) return await api.playPlaylist(page.items, 0, local, page.title);
		}
		if (music.playlistId) {
			const id = `VL${music.playlistId.replace(/^VL/, '')}`;
			const page = await api.getPlaylist(id);
			return await api.playPlaylist(page.items, 0, id, page.title, false, page.continuation);
		}
		if (music.seed) {
			// A random pick among the top results so the same vibe doesn't always start with the same song.
			const songs = (await api.search(music.seed)).slice(0, 5);
			const song = songs[Math.floor(Math.random() * songs.length)];
			if (song) return await api.startRadio('song', song.video_id, song.title);
		}
		toast.error(t('vibes.no_music'));
	} catch (e) {
		toast.error(String(e));
	}
}

let watching = false;
/** Follows Rust's `ui-visible` (hidden to the tray, which the page can't see for itself). */
export function watchUiVisible() {
	if (watching) return;
	watching = true;
	api.onUiVisible((v) => (vibes.uiVisible = v)).catch(() => {});
}
