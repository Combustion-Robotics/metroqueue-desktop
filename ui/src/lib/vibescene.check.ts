// Self-check for the vibe scene port in `vibescene.ts`:
//
//     node --experimental-strip-types ui/src/lib/vibescene.check.ts
//
// The hashes are of frames drawn by MetroQueue's Kotlin `SceneRenderer` (same spec, size and
// times), so a drift from the phone's picture, or a seed that lays out differently, fails here.
// Prints "ok" and exits 0, or throws on the first broken invariant.
import { SceneRenderer, VIBE_SCENES, readScene, type SceneSpec } from './vibescene.ts';

function ok(cond: boolean, what: string): void {
	if (!cond) throw new Error(`FAIL: ${what}`);
}

function frame(spec: SceneSpec, w: number, h: number, t: number): Int32Array {
	const r = new SceneRenderer(spec, w, h);
	// Kotlin draws at 0 on creation; the goldens then stepped to 1 s and on to `t`, which matters:
	// shooting stars and lightning draw from the same random stream as the layout.
	r.time = 1;
	r.render();
	r.time = t;
	r.render();
	return r.pixels;
}

const fnv = (px: Int32Array) => px.reduce((h, p) => Math.imul(h ^ p, 16777619), 0x811c9dc5) >>> 0;

const goldens: [string, SceneSpec, number, number, number, number][] = [
	['winter dusk', VIBE_SCENES.winterDusk, 320, 180, 4.2, 582887995],
	['rainy city', VIBE_SCENES.rainyCity, 320, 180, 4.2, 1848609318],
	['starry lake, portrait', VIBE_SCENES.starryLake, 180, 400, 9.35, 723048542],
	['spring blossoms, wide', VIBE_SCENES.springBlossoms, 427, 180, 9.35, 2701164695]
];
for (const [name, spec, w, h, t, want] of goldens) {
	const px = frame(spec, w, h, t);
	ok(fnv(px) === want, `${name} matches the Kotlin renderer (got ${fnv(px)})`);
	ok(px.every((p) => p >>> 24 === 0xff), `${name} is opaque`);
}

// A synced scene keeps what it knows and falls back to the defaults for the rest.
const read = readScene({ sky: 'Night', layout: 'Volcano', particles: ['Rain', 'Ash'], seed: 7.9, extra: 1 });
ok(read.sky === 'Night' && read.layout === 'Forest', 'unknown enum names read as the default');
ok(read.particles.join() === 'Rain' && read.seed === 7 && read.snowy, 'particles filtered, seed an Int');

console.log('ok');
