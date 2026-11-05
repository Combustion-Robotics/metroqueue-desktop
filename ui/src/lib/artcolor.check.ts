// Self-check for the artwork accent picker (`artcolor.ts`). No test runner in `ui/` (see
// color.check.ts) — node 22 runs TypeScript directly:
//
//     node --experimental-strip-types ui/src/lib/artcolor.check.ts
//
// Prints "ok" and exits 0, or throws on the first broken invariant.
import { pickAccent } from './artcolor.ts';
import { hexToHsv } from './color.ts';

/** An RGBA buffer: `n` pixels of each colour, in order. */
const buf = (...runs: [[number, number, number], number][]) =>
	new Uint8ClampedArray(runs.flatMap(([[r, g, b], n]) => Array.from({ length: n * 4 }, (_, i) => [r, g, b, 255][i % 4])));

const hue = (hex: string) => Math.round(hexToHsv(hex)!.h);

// A cover that is mostly black with a red band: area must not beat colour, or every dark cover
// themes the app grey.
const red = pickAccent(buf([[8, 8, 8], 900], [[200, 30, 30], 124]))!;
if (Math.abs(hue(red) - 0) > 20 && Math.abs(hue(red) - 360) > 20) throw new Error(`hue: ${red}`);

// Greyscale artwork keeps the user's theme instead of inventing a hue out of noise.
if (pickAccent(buf([[20, 20, 20], 512], [[210, 210, 212], 512])) !== null) throw new Error('grey');
if (pickAccent(new Uint8ClampedArray(0)) !== null) throw new Error('empty');

console.log('ok');
