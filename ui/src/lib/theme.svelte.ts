// The theme: one Material 3 colour scheme (m3.ts) from a seed colour, in light, dark or pure black,
// and the "looks" the UI reads directly (`appearance`). The seed is the user's colour, or the playing
// cover's while "Dynamic colour" is on.
//
// The three colour settings (seed, dynamic, pure black) are shared with the MetroQueue phone app
// (mqsync.rs, docs/metroqueue/SYNC.md), so their source of truth is the backend's settings table.
// localStorage keeps a copy only so the first frame paints in the right colours before the
// `get_settings` round-trip lands.
//
// The scheme is one generated <style> (light on :root, dark on :root.dark), so the light/dark
// switch stays a CSS class flip and a seed change is a single stylesheet write. Writing it restyles
// the whole document (docs/UI-PERFORMANCE.md): a cover change is one write, never per frame, and a
// colour drag in Settings previews on `ThemeMock` instead.

import { artworkAccent, warmAccent } from './artcolor';
import { getSettings, onSharedSettingsChanged, setSetting } from './api';
import { DEFAULT_SEED, argbIntToHex, hexToArgbInt, isHex, schemeSheet } from './m3';

const APPEARANCE_KEY = 'appearance';
const M3_KEY = 'm3-theme';
const STYLE_ID = 'm3-scheme';
// Backend setting keys, mapped to the phone's in mqsync.rs.
const SEED_SETTING = 'theme_seed'; // signed ARGB int, like the phone's `selectedThemeColor`
const DYNAMIC_SETTING = 'theme_dynamic';
const PURE_BLACK_SETTING = 'theme_pure_black';

/** The colour settings. `pureBlack` only changes dark mode, like on the phone. */
export const m3 = $state({ seed: DEFAULT_SEED, dynamic: false, pureBlack: false });

/** Enhanced lyrics' tuning (#591), what each reset icon in its settings card puts back. */
export const LYRICS_DEFAULTS = {
	enhancedLyricsLineSync: false,
	lyricsAnim: 'bounce' as LyricsAnim,
	lyricsStyle: 'feather' as LyricsStyle,
	lyricsAnimStrength: 100,
	lyricsGlow: 50
};

/**
 * Looks the UI reads directly rather than through a CSS token. localStorage only (a pure UI
 * preference, no backend round-trip), which also means a component can read it during its first
 * render instead of flashing the default while a command round-trips.
 */
export const appearance = $state({
	/** Blur the playing track's artwork behind the now-playing view. */
	artworkBackground: true,
	/** Blur the cover behind a playlist's header. Sharp, a square cover stretched to the page's
	 *  width is often too small for it and shows its pixels (#461). */
	blurPlaylistHeader: false,
	/** The "Now playing" pill a list floats over its rows while the playing one is off screen, in
	 *  the queue and on a playlist (#366). Off for whoever finds it in the way. */
	nowPlayingPill: true,
	/** The original now-playing view, before the redesign (title and album under the picture, a
	 *  lit backdrop, a full-height queue/lyrics column) became the default. A new key rather than
	 *  `stagePlayer` flipped: that one was saved as `false` for everyone, so a new default would
	 *  never have reached them. */
	classicPlayer: false,
	/**
	 * The now-playing view carries queue and lyrics itself, as tabs, and the player bar's two
	 * buttons switch between them while it's open. Off, those buttons only ever open the floating
	 * side panels, which then sit over the now-playing view like they sit over a page (#62).
	 */
	tabbedPlayer: true,
	/** Starting playback opens the now-playing view. Off, it plays and leaves you where you are (#64). */
	openPlayerOnPlay: true,
	/** A click on empty player-bar space toggles the now-playing view, and one around the artwork
	 *  closes it (#302). Off, only the buttons and shortcuts do, for whoever trips it by accident. */
	emptyClickTogglesPlayer: true,
	/** Theater mode's second and third columns (#297). Sticky, like every view toggle. */
	theaterLyrics: true,
	theaterQueue: false,
	enhancedLyrics: false,
	...LYRICS_DEFAULTS
});

export type LyricsAnim = 'bounce' | 'float' | 'wave' | 'pop' | 'slide' | 'none';
export type LyricsStyle = 'feather' | 'accent' | 'crisp' | 'word';
export const LYRICS_ANIMS: LyricsAnim[] = ['bounce', 'float', 'wave', 'pop', 'slide', 'none'];
export const LYRICS_STYLES: LyricsStyle[] = ['feather', 'accent', 'crisp', 'word'];

export function setAppearance(patch: Partial<typeof appearance>): void {
	Object.assign(appearance, patch);
	localStorage.setItem(APPEARANCE_KEY, JSON.stringify(appearance));
}

/** Change colour settings: applied, cached for the next launch, and saved (so synced) in the backend. */
export function setM3(patch: Partial<typeof m3>): void {
	Object.assign(m3, patch);
	localStorage.setItem(M3_KEY, JSON.stringify(m3));
	const save = (key: string, value: string) => setSetting(key, value).catch(() => {});
	if (patch.seed !== undefined) save(SEED_SETTING, String(hexToArgbInt(m3.seed)));
	if (patch.dynamic !== undefined) save(DYNAMIC_SETTING, String(m3.dynamic));
	if (patch.pureBlack !== undefined) save(PURE_BLACK_SETTING, String(m3.pureBlack));
	apply();
}

/** Only well-formed values: localStorage can be hand-edited, the backend's can come from a phone. */
function adopt(saved: { seed?: unknown; dynamic?: unknown; pureBlack?: unknown }): void {
	if (isHex(saved.seed)) m3.seed = saved.seed.toLowerCase();
	if (typeof saved.dynamic === 'boolean') m3.dynamic = saved.dynamic;
	if (typeof saved.pureBlack === 'boolean') m3.pureBlack = saved.pureBlack;
}

/** Take what the backend holds (startup, and after a sync with the phone changed it). */
async function loadBackend(): Promise<void> {
	const s = await getSettings().catch(() => ({}) as Record<string, string>);
	const bool = (v: string | undefined) => (v === 'true' ? true : v === 'false' ? false : undefined);
	const seed = Number(s[SEED_SETTING]);
	const before = JSON.stringify(m3);
	adopt({
		seed: s[SEED_SETTING] && Number.isInteger(seed) ? argbIntToHex(seed) : undefined,
		dynamic: bool(s[DYNAMIC_SETTING]),
		pureBlack: bool(s[PURE_BLACK_SETTING])
	});
	if (JSON.stringify(m3) === before) return;
	localStorage.setItem(M3_KEY, JSON.stringify(m3));
	apply();
}

// The playing cover's colour while "Dynamic colour" is on. Not persisted: the next track overwrites it.
let art: string | null = null;
let wanted = '';
let current = '';

/**
 * Write the scheme for the current seed. `animate` crossfades the change (a cover change) with a
 * view transition: two snapshots blended on the compositor, the cheapest way to fade every colour
 * at once, where transitioning the tokens themselves restyles the document every frame (#217).
 */
function apply(animate = false): void {
	const css = schemeSheet((m3.dynamic && art) || m3.seed, m3.pureBlack);
	if (css === current) return; // a repeat or a cover with the same colour: no restyle
	current = css;
	let style = document.getElementById(STYLE_ID);
	if (!style) {
		style = document.createElement('style');
		style.id = STYLE_ID;
		document.head.prepend(style);
	}
	const el = style;
	const write = () => (el.textContent = css);
	const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
	if (animate && document.startViewTransition && !still) document.startViewTransition(write);
	else write();
}

// The window can be closed to the tray with playback carrying on, and WebKitGTK does not tell the
// page (`document.visibilityState` stays "visible"), so Rust does: `ui-visible`, from tray.rs.
// Restyling the document for a window nobody can see is not just wasted work: the web process
// holds on to every one of those restyles until the window is mapped again, so a night in the tray
// grows unbounded and then hands it all back the moment the user opens the app.
let uiVisible = true;
let restyleWanted = false;

/** Skip the artwork restyle while the window is hidden; run the last one when it comes back. */
function restyle(): void {
	if (!uiVisible) {
		restyleWanted = true;
		return;
	}
	apply(true);
}

export function setUiVisible(visible: boolean): void {
	uiVisible = visible;
	if (visible && restyleWanted) {
		restyleWanted = false;
		apply();
	}
}

/**
 * Point the theme at a cover URL (only while "Dynamic colour" is on). `null`/undefined (setting off,
 * nothing playing) goes back to the user's seed. An unreadable or colourless cover does too once
 * decoded: keeping the last track's colour paints this one in a cover it doesn't have (#522).
 */
export function applyArtworkAccent(url: string | undefined | null): void {
	wanted = url ?? '';
	if (!url) {
		if (art) {
			art = null;
			restyle();
		}
		return;
	}
	artworkAccent(url).then((hex) => {
		if (wanted !== url || art === hex) return; // a faster track change won, or the same colour
		art = hex;
		restyle();
	});
}

/** Decode a cover the user is about to hear, so its colour is ready the instant the track flips. */
export function prewarmArtworkAccent(url: string | undefined | null): void {
	if (url) warmAccent(url);
}

/** Apply the cached theme on startup (before the first paint), then reconcile with the backend. */
export function initTheme(): void {
	try {
		adopt(JSON.parse(localStorage.getItem(M3_KEY) ?? '{}') ?? {});
	} catch {
		// unparseable — defaults
	}
	// The old palette presets, custom accent/tint/roundness and fonts are gone; their keys are dropped.
	localStorage.removeItem('primary-theme');
	localStorage.removeItem('custom-theme');
	try {
		const saved = JSON.parse(localStorage.getItem(APPEARANCE_KEY) ?? '{}');
		for (const k of [
			'artworkBackground',
			'blurPlaylistHeader',
			'nowPlayingPill',
			'classicPlayer',
			'tabbedPlayer',
			'openPlayerOnPlay',
			'emptyClickTogglesPlayer',
			'theaterLyrics',
			'theaterQueue',
			'enhancedLyrics',
			'enhancedLyricsLineSync'
		] as const) {
			if (typeof saved?.[k] === 'boolean') appearance[k] = saved[k];
		}
		if (LYRICS_ANIMS.includes(saved?.lyricsAnim)) appearance.lyricsAnim = saved.lyricsAnim;
		if (LYRICS_STYLES.includes(saved?.lyricsStyle)) appearance.lyricsStyle = saved.lyricsStyle;
		if (typeof saved?.lyricsAnimStrength === 'number') {
			appearance.lyricsAnimStrength = Math.min(200, Math.max(25, saved.lyricsAnimStrength));
		}
		if (typeof saved?.lyricsGlow === 'number') {
			appearance.lyricsGlow = Math.min(100, Math.max(0, saved.lyricsGlow));
		}
	} catch {
		// unparseable — keep the defaults
	}
	apply();
	loadBackend();
	onSharedSettingsChanged(loadBackend).catch(() => {});
	// The other window (main / mini player) changed them: same origin, so this fires there.
	window.addEventListener('storage', (e) => {
		if (e.key !== M3_KEY) return;
		try {
			adopt(JSON.parse(e.newValue ?? '{}') ?? {});
			apply();
		} catch {
			// ignore a garbled write
		}
	});
}
