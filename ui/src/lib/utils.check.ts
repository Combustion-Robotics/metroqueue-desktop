// Self-check for `cn` in `utils.ts`: the M3 type scale merges as a font size, not a colour.
//
//     node --experimental-strip-types ui/src/lib/utils.check.ts
//
// Prints "ok" and exits 0, or throws on the first broken invariant.
import { cn } from './utils.ts';

const eq = (a: unknown, b: unknown, msg: string) => {
	if (a !== b) throw new Error(`${msg}: ${a} !== ${b}`);
};

eq(cn('text-title-sm text-on-surface-variant'), 'text-title-sm text-on-surface-variant', 'size kept beside a colour');
eq(cn('text-headline-sm text-on-surface', 'text-base'), 'text-on-surface text-base', 'a caller size still wins');
eq(cn('text-body-md', 'text-label-lg'), 'text-label-lg', 'two scale sizes collapse to the last');

console.log('ok');
