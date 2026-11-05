// The Material 3 colour scheme, the one source of every colour token in the app. A seed colour (the
// user's, or the playing cover's) becomes the full set of M3 roles through Google's own
// material-color-utilities, the same TonalSpot / 2025 spec the MetroQueue phone app themes itself
// with, so a seed synced from the phone looks the same here. layout.css maps the shadcn tokens
// (--background, --primary, …) onto these roles.
import {
	Hct,
	MaterialDynamicColors,
	SchemeTonalSpot,
	argbFromHex,
	hexFromArgb
} from '@material/material-color-utilities';

/** MetroQueue's default seed (`DefaultThemeColor` on the phone). */
export const DEFAULT_SEED = '#ed5564';

const ROLES = new MaterialDynamicColors().allColors;
const HEX = /^#[0-9a-f]{6}$/i;

export const isHex = (v: unknown): v is string => typeof v === 'string' && HEX.test(v);

/** The phone stores the seed as a signed 32-bit ARGB int (sync key `selectedThemeColor`). */
export const hexToArgbInt = (hex: string) => argbFromHex(hex) | 0;
export const argbIntToHex = (n: number) => hexFromArgb((n | 0xff000000) >>> 0);

// Pure black: the large surfaces (page, shell, rail) go to #000 so OLED pixels switch off, and the
// containers on top of them keep a few tones of lift so cards and menus still read as raised.
const BLACK = [
	'background',
	'surface',
	'surface-dim',
	'surface-container-lowest',
	'surface-container-low'
];
const LIFT: [string, number][] = [
	['surface-container', 6],
	['surface-container-high', 10],
	['surface-container-highest', 14],
	['surface-bright', 18]
];

/** Every M3 colour role for a seed, as `--md-sys-color-<role>` → hex. */
export function schemeVars(seed: string, dark: boolean, pureBlack = false): Record<string, string> {
	const source = Hct.fromInt(argbFromHex(isHex(seed) ? seed : DEFAULT_SEED));
	const s = new SchemeTonalSpot(source, dark, 0, '2025');
	const vars: Record<string, string> = {};
	for (const role of ROLES) {
		vars[`--md-sys-color-${role.name.replaceAll('_', '-')}`] = hexFromArgb(role.getArgb(s));
	}
	if (dark && pureBlack) {
		for (const r of BLACK) vars[`--md-sys-color-${r}`] = '#000000';
		for (const [r, t] of LIFT) vars[`--md-sys-color-${r}`] = hexFromArgb(s.neutralPalette.tone(t));
	}
	return vars;
}

export const declarations = (vars: Record<string, string>) =>
	Object.entries(vars)
		.map(([k, v]) => `${k}:${v}`)
		.join(';');

// shortcut: unbounded only in theory, one entry per seed seen (covers repeat, the cache is cleared
// past 64); swap for an LRU if a session ever cycles that many seeds and the rebuild shows up.
const sheets = new Map<string, string>();

/** The stylesheet for a seed: light on :root, dark on :root.dark (mode-watcher's class). */
export function schemeSheet(seed: string, pureBlack: boolean): string {
	const key = `${seed}|${pureBlack}`;
	let css = sheets.get(key);
	if (!css) {
		const light = declarations(schemeVars(seed, false));
		const dark = declarations(schemeVars(seed, true, pureBlack));
		css = `:root{${light}}:root.dark{${dark}}`;
		if (sheets.size > 64) sheets.clear();
		sheets.set(key, css);
	}
	return css;
}
