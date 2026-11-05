// Self-check for the M3 scheme (`m3.ts`), run by `pnpm test` (run-checks.mjs).
//
// material-color-utilities 0.4.0 has a few extensionless relative imports that bundlers resolve and
// Node does not, so this check adds the `.js` itself before importing it.
// @ts-ignore: a Node-only check, and the DOM-only tsconfig has no @types/node
import { register } from 'node:module';
register(
	'data:text/javascript,export async function resolve(s,c,n){try{return await n(s,c)}catch(e){if(s.startsWith(".")&&!s.endsWith(".js"))return n(s+".js",c);throw e}}'
);
const { schemeVars, schemeSheet, hexToArgbInt, argbIntToHex, DEFAULT_SEED } = await import('./m3.ts');

const light = schemeVars(DEFAULT_SEED, false);
const dark = schemeVars(DEFAULT_SEED, true);
const black = schemeVars(DEFAULT_SEED, true, true);
const lum = (hex: string) => parseInt(hex.slice(1, 3), 16) + parseInt(hex.slice(3, 5), 16) + parseInt(hex.slice(5, 7), 16);

for (const role of ['primary', 'on-primary', 'primary-container', 'secondary-container', 'tertiary', 'error', 'surface', 'surface-container-highest', 'on-surface-variant', 'outline-variant', 'inverse-surface']) {
	if (!/^#[0-9a-f]{6}$/.test(light[`--md-sys-color-${role}`] ?? '')) throw new Error(`missing ${role}`);
}
if (lum(light['--md-sys-color-surface']) < lum(dark['--md-sys-color-surface'])) throw new Error('light darker than dark');
if (black['--md-sys-color-surface'] !== '#000000' || black['--md-sys-color-surface-container-low'] !== '#000000') throw new Error('pure black');
// The containers stay lifted off black, in order, or cards vanish into the page.
const tones = ['surface-container', 'surface-container-high', 'surface-container-highest'].map((r) => lum(black[`--md-sys-color-${r}`]));
if (!(tones[0] > 0 && tones[0] < tones[1] && tones[1] < tones[2])) throw new Error(`lift: ${tones}`);
// Pure black never touches light mode.
if (!schemeSheet(DEFAULT_SEED, true).startsWith(`:root{--md-sys-color-background:${light['--md-sys-color-background']}`)) throw new Error('light sheet');

// Round trip with the phone's signed ARGB int: #ED5564 is 0xFFED5564.
if (hexToArgbInt('#ed5564') !== (0xffed5564 | 0)) throw new Error('to int');
if (argbIntToHex(0xffed5564 | 0) !== '#ed5564' || argbIntToHex(-16776961) !== '#0000ff') throw new Error('from int');

console.log('ok');
