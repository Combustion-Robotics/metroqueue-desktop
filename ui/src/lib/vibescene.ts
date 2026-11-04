// The pixel-art vibe scene engine, ported line by line from MetroQueue's
// `ui/vibe/scene/SceneSpec.kt` + `SceneRenderer.kt` so a vibe synced from the phone draws the same
// picture here. Kotlin draws in 32-bit floats and Int arithmetic: `fi` (Float -> Int), `idiv` and
// the `fround`s in `mix` keep the results identical where it shows; `vibescene.check.ts` compares
// against pixels from the Kotlin renderer. Plain TS (no DOM), so the check runs in Node.

export const SKIES = ['Dusk', 'Sunset', 'Day', 'Night', 'NeonNight'] as const;
export const LAYOUTS = ['Forest', 'Lake', 'City', 'Blossoms', 'Hills'] as const;
export const FOREGROUNDS = ['None', 'Rails', 'Shore', 'Rooftops', 'Window'] as const;
export const PARTICLES = [
	'Snow',
	'Petals',
	'Rain',
	'Fireflies',
	'Stars',
	'ShootingStars',
	'Clouds',
	'Lightning'
] as const;
export type Sky = (typeof SKIES)[number];
export type Layout = (typeof LAYOUTS)[number];
export type Foreground = (typeof FOREGROUNDS)[number];
export type Particle = (typeof PARTICLES)[number];

/** MetroQueue's `SceneSpec` JSON. */
export interface SceneSpec {
	sky: Sky;
	layout: Layout;
	foreground: Foreground;
	particles: Particle[];
	snowy: boolean;
	seed: number;
}

export const DEFAULT_SCENE: SceneSpec = {
	sky: 'Dusk',
	layout: 'Forest',
	foreground: 'Rails',
	particles: ['Snow'],
	snowy: true,
	seed: 1
};

export const VIBE_SCENES = {
	winterDusk: DEFAULT_SCENE,
	rainyCity: {
		sky: 'NeonNight',
		layout: 'City',
		foreground: 'Rooftops',
		particles: ['Rain', 'Lightning'],
		snowy: false,
		seed: 3
	},
	starryLake: {
		sky: 'Night',
		layout: 'Lake',
		foreground: 'Shore',
		particles: ['Stars', 'ShootingStars', 'Fireflies'],
		snowy: false,
		seed: 5
	},
	springBlossoms: {
		sky: 'Day',
		layout: 'Blossoms',
		foreground: 'None',
		particles: ['Petals', 'Clouds'],
		snowy: false,
		seed: 2
	}
} satisfies Record<string, SceneSpec>;

/** A stored scene with MetroQueue's decoding rules: missing or unknown values take the defaults. */
export function readScene(raw: unknown): SceneSpec {
	const o = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
	const pick = <T extends string>(v: unknown, all: readonly T[], d: T): T =>
		all.includes(v as T) ? (v as T) : d;
	return {
		sky: pick(o.sky, SKIES, DEFAULT_SCENE.sky),
		layout: pick(o.layout, LAYOUTS, DEFAULT_SCENE.layout),
		foreground: pick(o.foreground, FOREGROUNDS, DEFAULT_SCENE.foreground),
		particles: Array.isArray(o.particles)
			? PARTICLES.filter((p) => (o.particles as unknown[]).includes(p))
			: DEFAULT_SCENE.particles,
		snowy: typeof o.snowy === 'boolean' ? o.snowy : DEFAULT_SCENE.snowy,
		seed: typeof o.seed === 'number' ? o.seed | 0 : DEFAULT_SCENE.seed
	};
}

type Body = 'None' | 'Moon' | 'Sun';
interface SkyDef {
	bands: number[];
	land: number;
	ground: number;
	accent: number;
	glow: number;
	body: Body;
	bodyX: number;
	bodyY: number;
	night: boolean;
}

// Time of day + palette. Colours are 0xRRGGBB; bands run from the top of the sky down to the horizon.
const SKY_DEFS: Record<Sky, SkyDef> = {
	Dusk: sky([0x8c8ed6, 0x9c97da, 0xb09fd9, 0xc9a7d6, 0xe0b0cf, 0xf3bfc6], 0x67639a, 0x8e86b4, 0xf7b3c7, 0xf8dde8, 'Moon', 0.76, 0.15, false),
	Sunset: sky([0x3e3a78, 0x6a4790, 0xa0559c, 0xd8699a, 0xf39089, 0xfbc08f], 0x3b2d5e, 0x5c4a6e, 0xffb38a, 0xffe2a8, 'Sun', 0.62, 0.5, false),
	Day: sky([0x8ec2ee, 0xa3cdf2, 0xbad8f4, 0xd2e2f4, 0xeae6f2, 0xf8e3ec], 0x5e9c7e, 0x9ccb8e, 0xf7b6cf, 0xfff8dc, 'Sun', 0.22, 0.13, false),
	Night: sky([0x0a0e2c, 0x10163a, 0x171f4a, 0x212a5c, 0x2f376c, 0x45467e], 0x0e1230, 0x1c2244, 0xffe9a8, 0xf4f0d8, 'Moon', 0.7, 0.14, true),
	NeonNight: sky([0x110d26, 0x1a1538, 0x271c4c, 0x3d2462, 0x5e2d74, 0x8c3c82], 0x140f2a, 0x1e1838, 0xff6fb5, 0xf2e6f0, 'None', 0.5, 0.2, true)
};

function sky(bands: number[], land: number, ground: number, accent: number, glow: number, body: Body, bodyX: number, bodyY: number, night: boolean): SkyDef {
	return { bands, land, ground, accent, glow, body, bodyX: Math.fround(bodyX), bodyY: Math.fround(bodyY), night };
}

/** `kotlin.random.Random(seed)` (XorWow), so a seed lays a scene out exactly as on the phone. */
export class KotlinRandom {
	private x: number;
	private y: number;
	private z = 0;
	private w = 0;
	private v: number;
	private addend: number;

	constructor(seed: number) {
		const s2 = seed >> 31;
		this.x = seed | 0;
		this.y = s2;
		this.v = ~seed;
		this.addend = (seed << 10) ^ (s2 >>> 4);
		for (let i = 0; i < 64; i++) this.nextInt();
	}

	nextInt(): number {
		let t = this.x;
		t ^= t >>> 2;
		this.x = this.y;
		this.y = this.z;
		this.z = this.w;
		const v0 = this.v;
		this.w = v0;
		t = t ^ (t << 1) ^ v0 ^ (v0 << 4);
		this.v = t;
		this.addend = (this.addend + 362437) | 0;
		return (t + this.addend) | 0;
	}

	private bits(n: number): number {
		return (this.nextInt() >>> (32 - n)) & (-n >> 31);
	}

	/** `nextInt(until)` / `nextInt(from, until)`. */
	nextIn(from: number, until?: number): number {
		if (until === undefined) [from, until] = [0, from];
		const n = (until - from) | 0;
		if ((n & -n) === n) return from + this.bits(31 - Math.clz32(n));
		let bits: number;
		let v: number;
		do {
			bits = this.nextInt() >>> 1;
			v = bits % n;
		} while (((bits - v + (n - 1)) | 0) < 0);
		return from + v;
	}

	nextFloat(): number {
		return this.bits(24) / 16777216;
	}

	nextDouble(): number {
		return (this.bits(26) * 134217728 + this.bits(27)) / 9007199254740992;
	}

	nextBoolean(): boolean {
		return this.bits(1) !== 0;
	}
}

const f = Math.fround;
/** Kotlin's `Float.toInt()`. */
const fi = (v: number) => f(v) | 0;
const idiv = (a: number, b: number) => (a / b) | 0;
const clamp = (v: number, lo: number, hi: number) => (v < lo ? lo : v > hi ? hi : v);

const SKY_D = 0;
const MOON = 1;
const WATER = 2;
const LAND = 3;
const FRONT = 4;
const OPAQUE = -0x1000000;
const WHITE = 0xffffff;
const INK = 0x1b1530;
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
const BLOSSOM = [0xb45a8c, 0xd67aa5, 0xec9cbf, 0xf7bed4, 0xffe3ee];
const NEON = [0xff6fb5, 0x6fe8ff, 0xb98cff];
const BULBS = [0xffd27a, 0xff8fb8, 0x8fe8ff, 0xffb36b];
const WINDOWS = [0xffd27a, 0xffb86b, 0xffe8b0, 0xffd27a, 0x9fd3ff];
const WINDOW_LIGHT = 0xffd27a;
const BEACON = 0xff4d6d;
const CAT = ['1.1....', '111....', '212....', '111....', '.11...1', '1111..1', '11111.1', '1111111', '111111.'];
const PI = Math.PI;

export function hash(x: number, y: number, seed: number): number {
	let h = (Math.imul(x, 374761393) + Math.imul(y, 668265263) + Math.imul(seed, 1442695041)) | 0;
	h = Math.imul(h ^ (h >>> 13), 1274126177);
	return ((h ^ (h >>> 16)) & 0xffffff) / 16777216;
}

const smooth = (t: number) => t * t * (3 - 2 * t);
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

function noise2(x: number, y: number, seed: number): number {
	const i = Math.floor(x);
	const j = Math.floor(y);
	const u = smooth(x - i);
	const a = lerp(hash(i, j, seed), hash(i + 1, j, seed), u);
	const b = lerp(hash(i, j + 1, seed), hash(i + 1, j + 1, seed), u);
	return lerp(a, b, smooth(y - j));
}

const fbm = (x: number, seed: number) =>
	noise2(x, 0, seed) * 0.6 + noise2(x * 2.3, 0, seed + 1) * 0.28 + noise2(x * 5.1, 0, seed + 2) * 0.12;

/** Blends two 0xRRGGBB colours in Kotlin's float steps; the result is always opaque. */
function mix(a: number, b: number, t: number): number {
	t = f(t);
	const ar = (a >> 16) & 255;
	const ag = (a >> 8) & 255;
	const ab = a & 255;
	return (
		OPAQUE |
		((f(ar + f((((b >> 16) & 255) - ar) * t)) | 0) << 16) |
		((f(ag + f((((b >> 8) & 255) - ag) * t)) | 0) << 8) |
		(f(ab + f(((b & 255) - ab) * t)) | 0)
	);
}

const bayer = (x: number, y: number) => (BAYER[(y & 3) * 4 + (x & 3)] + 0.5) / 16;

/** Ordered-dithers a 0..1 amount to sixths, so soft blends still read as crisp pixel steps. */
const dq = (a: number, x: number, y: number) => f(Math.min(Math.floor(f(f(clamp(a, 0, 1) * 6) + bayer(x, y))), 6) / 6);

/** Dithered gradient through `c` (t = 0..1) with three in-between tones per step. */
function ramp(c: number[], t: number, x: number, y: number): number {
	const n = c.length - 1;
	const k = Math.min(fi(clamp(t, 0, 1) * n * 4 + bayer(x, y)), n * 4);
	const i = Math.min(idiv(k, 4), n - 1);
	return mix(c[i], c[i + 1], (k - i * 4) / 4);
}

function wrap(v: number, m: number): number {
	const r = v % m;
	return (r < 0 ? r + m : r) | 0;
}

interface Flock {
	n: number;
	x: Float32Array;
	y: Float32Array;
	v: Float32Array;
	p: Float32Array;
}

/**
 * Paints a scene into a low-res ARGB buffer. Static layers are painted once into `base`; `render`
 * copies them into `pixels` and adds the animated bits for `time` without allocating.
 */
export class SceneRenderer {
	readonly pixels: Int32Array;
	readonly w: number;
	readonly h: number;
	time = 0;

	private readonly base: Int32Array;
	private readonly depth: Uint8Array;
	private readonly spec: SceneSpec;
	private readonly sky: SkyDef;
	private readonly seed: number;
	private readonly rnd: KotlinRandom;
	private readonly portrait: boolean;
	private readonly has: (p: Particle) => boolean;

	// Sizes are in scene pixels; the short side is ~180 so things keep their size in both orientations.
	private readonly horizon: number;
	private readonly haze: number;
	private readonly near: number;
	private readonly snow: number;
	private readonly snowHi: number;
	private readonly snowShade: number;
	private readonly lightDir: number;
	private readonly lightsOn: boolean;
	private readonly litP: number;
	private readonly bodyCx: number;
	private readonly bodyCy: number;
	private readonly bodyR: number;
	private stamp = 0;

	private readonly snowFlock: Flock | null;
	private readonly petalFlock: Flock | null;
	private readonly rainFlock: Flock | null;
	private readonly fireflyFlock: Flock | null;
	private readonly stars: Int32Array;
	private readonly starA: Float32Array;
	private starCount = 0;
	private readonly beacons = new Int32Array(16);
	private beaconCount = 0;
	private readonly beaconDim: number;
	private readonly chimneys = new Int32Array(4);
	private chimneyCount = 0;

	private readonly cloudN: number;
	private readonly cloudW: number[] = [];
	private readonly cloudH: number[] = [];
	private readonly cloudX: number[] = [];
	private readonly cloudY: number[] = [];
	private readonly cloudV: number[] = [];
	private readonly cloudPix: Uint8Array[] = [];
	private readonly cloudTones: number[];

	private shootAt: number;
	private shootX = 0;
	private shootY = 0;
	private shootDx = 0;
	private shootDy = 0;
	private boltAt: number;
	private readonly bolt: Int32Array;
	private boltLen = 0;

	constructor(spec: SceneSpec, w: number, h: number) {
		this.spec = spec;
		this.w = w;
		this.h = h;
		this.pixels = new Int32Array(w * h);
		this.base = new Int32Array(w * h);
		this.depth = new Uint8Array(w * h);
		const sky = (this.sky = SKY_DEFS[spec.sky]);
		this.seed = spec.seed | 0;
		this.rnd = new KotlinRandom(this.seed);
		this.portrait = h > w;
		this.has = (p) => spec.particles.includes(p);

		this.horizon = h - Math.max(60, fi(h * f(0.3)));
		this.haze = sky.bands[4];
		this.near = sky.land;
		this.snow = mix(WHITE, sky.bands[1], sky.night ? 0.55 : 0.12);
		this.snowHi = mix(this.snow, sky.glow, sky.night ? 0.12 : 0.4);
		this.snowShade = mix(this.snow, sky.bands[0], sky.night ? 0.35 : 0.45);
		this.lightDir = sky.bodyX >= 0.5 ? 1 : -1;
		this.lightsOn = spec.sky !== 'Day';
		this.litP = sky.night ? 0.42 : spec.sky === 'Day' ? 0.04 : 0.28;
		this.bodyCx = fi(w * sky.bodyX);
		this.bodyCy = fi(h * sky.bodyY);
		this.bodyR = sky.body === 'Sun' ? (sky.bodyY > f(0.4) ? 15 : 9) : 9;

		this.snowFlock = this.flock('Snow', idiv(w * h, 300));
		this.petalFlock = this.flock('Petals', idiv(w * h, 800));
		this.rainFlock = this.flock('Rain', idiv(w * h, 380));
		this.fireflyFlock = this.flock('Fireflies', 14 + idiv(w * h, 6000));
		this.stars = new Int32Array(this.has('Stars') ? idiv(w * this.horizon, 200) : 0);
		this.starA = new Float32Array(this.stars.length);
		this.beaconDim = mix(BEACON, this.near, 0.5);

		const rnd = this.rnd;
		const n = (this.cloudN = this.has('Clouds') ? 4 : 0);
		for (let i = 0; i < n; i++) this.cloudW.push(30 + rnd.nextIn(34));
		for (let i = 0; i < n; i++) this.cloudH.push(12 + rnd.nextIn(6));
		for (let i = 0; i < n; i++) this.cloudX.push(f(rnd.nextFloat() * w));
		for (let i = 0; i < n; i++) this.cloudY.push(fi(h * f(0.05) + f(f(rnd.nextFloat() * this.horizon) * f(0.4))));
		for (let i = 0; i < n; i++) this.cloudV.push(f(1.2 + f(rnd.nextFloat() * 2.5)));
		for (let i = 0; i < n; i++) this.cloudPix.push(this.cloudSprite(this.cloudW[i], this.cloudH[i]));
		const body = mix(sky.bands[1], WHITE, sky.night ? 0.12 : 0.6);
		this.cloudTones = [0, mix(body, sky.bands[0], 0.3), body, mix(body, sky.night ? sky.glow : WHITE, 0.45)];

		this.shootAt = 3.0 + rnd.nextDouble() * 6;
		this.boltAt = 6.0 + rnd.nextDouble() * 10;
		this.bolt = new Int32Array(h);

		this.paintSky();
		switch (spec.layout) {
			case 'Forest': this.forest(); break;
			case 'Lake': this.lake(); break;
			case 'City': this.city(); break;
			case 'Blossoms': this.blossoms(); break;
			case 'Hills': this.hills(); break;
		}
		switch (spec.foreground) {
			case 'Rails': this.rails(); break;
			case 'Shore': this.shore(); break;
			case 'Rooftops': this.rooftops(); break;
			case 'Window': this.window(); break;
		}
		const stars = this.stars;
		for (let r = 0; r < stars.length * 3; r++) {
			if (this.starCount === stars.length) continue;
			const y = rnd.nextIn(Math.max(fi(this.horizon * f(0.85)), 1));
			const i = y * w + rnd.nextIn(w);
			if (this.depth[i] !== SKY_D) continue;
			// Fainter towards the brighter sky near the horizon; a few bright ones.
			this.starA[this.starCount] = (1 - f(y / this.horizon)) * (rnd.nextFloat() < f(0.15) ? 1 : 0.6);
			stars[this.starCount++] = i;
		}
		this.aimShootingStar();
		this.render();
	}

	render(): void {
		const t = this.time;
		this.pixels.set(this.base);
		if (this.spec.layout === 'Lake') this.shimmer(t);
		this.drawStars(t);
		if (this.has('ShootingStars')) this.shootingStar(t);
		this.drawClouds(t);
		this.blink(t);
		this.smoke(t);
		if (this.has('Lightning')) this.lightning(t);
		if (this.snowFlock) this.snowfall(this.snowFlock, t);
		if (this.petalFlock) this.petals(this.petalFlock, t);
		if (this.rainFlock) this.rain(this.rainFlock, t);
		if (this.fireflyFlock) this.fireflies(this.fireflyFlock, t);
	}

	// region static painting

	private put(x: number, y: number, c: number, d = LAND): void {
		if (x < 0 || x >= this.w || y < 0 || y >= this.h) return;
		const i = y * this.w + x;
		this.base[i] = c | OPAQUE;
		this.depth[i] = d;
	}

	private tint(x: number, y: number, c: number, t: number): void {
		if (t > 0 && x >= 0 && x < this.w && y >= 0 && y < this.h) {
			const i = y * this.w + x;
			this.base[i] = mix(this.base[i], c, t);
		}
	}

	private rect(x: number, y: number, rw: number, rh: number, c: number, d = LAND): void {
		for (let yy = y; yy < y + rh; yy++) for (let xx = x; xx < x + rw; xx++) this.put(xx, yy, c, d);
	}

	private line(x0: number, y0: number, x1: number, y1: number, c: number): void {
		const n = Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0), 1);
		for (let k = 0; k <= n; k++) this.put(x0 + idiv((x1 - x0) * k, n), y0 + idiv((y1 - y0) * k, n), c);
	}

	/** Soft round light (window glow, lamp halo) faded with ordered dithering. */
	private halo(cx: number, cy: number, r: number, c: number, a: number): void {
		for (let dy = -r; dy <= r; dy++)
			for (let dx = -r; dx <= r; dx++) {
				const d = f(1 - f(f(Math.sqrt(dx * dx + dy * dy)) / r));
				if (d > 0) this.tint(cx + dx, cy + dy, c, dq(f(f(d * d) * f(a)), cx + dx, cy + dy));
			}
	}

	/** A band of ground mist tinting everything towards the horizon colour. */
	private mist(cy: number, half: number, a: number): void {
		for (let y = cy - half; y <= cy + half; y++) {
			const k = f(a) * f(1 - f(Math.abs(y - cy) / (half + 1)));
			for (let x = 0; x < this.w; x++)
				this.tint(x, y, this.haze, dq(k * (0.6 + 0.8 * noise2(x * 0.04, y * 0.2, this.seed + 77)), x, y));
		}
	}

	private paintSky(): void {
		const { sky, w, h, horizon, bodyCx, bodyCy, bodyR } = this;
		const lowSun = sky.body === 'Sun' && sky.bodyY > f(0.4);
		const glowR = lowSun ? f(w * f(0.6)) : sky.night ? 24 + bodyR * 2 : 34 + bodyR * 3;
		const glowA = sky.body === 'None' ? 0 : sky.night ? 0.18 : lowSun ? 0.75 : 0.5;
		for (let y = 0; y < h; y++)
			for (let x = 0; x < w; x++) {
				let c = ramp(sky.bands, f(y / horizon), x, y);
				if (glowA > 0) {
					const dx = x - bodyCx;
					const dy = f((y - bodyCy) * (lowSun ? f(2.2) : 1));
					const g = f(1 - f(f(Math.sqrt(f(f(dx * dx) + f(dy * dy)))) / glowR));
					if (g > 0) c = mix(c, sky.glow, dq(f(f(g * g) * f(glowA)), x, y));
				}
				this.put(x, y, c, SKY_D);
			}
		if (sky.body !== 'None') this.celestial();
		if (!sky.night) this.wisps();
	}

	private celestial(): void {
		const { sky, bodyCx, bodyCy, seed } = this;
		const r = this.bodyR;
		const g = sky.glow;
		for (let dy = -r; dy <= r; dy++)
			for (let dx = -r; dx <= r; dx++) {
				const d2 = dx * dx + dy * dy;
				if (d2 > r * r + r) continue;
				const x = bodyCx + dx;
				const y = bodyCy + dy;
				let c: number;
				if (sky.body === 'Sun') {
					c = mix(mix(g, WHITE, 0.5), mix(g, sky.accent, 0.6), dq(f((dy + r) / (2 * r)), x, y));
				} else {
					// Lit from the upper right, with a soft terminator, a few darker maria and a bright limb.
					const light = f(f(f(dx * f(0.55)) - f(dy * f(0.65))) / r);
					let m = mix(g, sky.bands[2], f(dq(f(f(-0.15) - light), x, y) * f(0.6)));
					if (noise2(dx * 0.42 + 3, dy * 0.42 + 3, seed + 50) > 0.62) m = mix(m, sky.bands[2], 0.22);
					if (d2 >= (r - 1) * (r - 1) && light > f(0.2)) m = mix(m, WHITE, 0.45);
					c = m;
				}
				this.put(x, y, c, MOON);
			}
	}

	/** Thin streaks of cloud lit by the low sun, like brush strokes across the sky. */
	private wisps(): void {
		const { sky, w, horizon, seed } = this;
		const c = this.spec.sky === 'Day' ? WHITE : mix(sky.glow, sky.accent, 0.4);
		const y0 = fi(horizon * f(0.1));
		for (let y = y0; y < horizon - 2; y++) {
			const env = Math.sin((PI * (y - y0)) / (horizon - y0));
			for (let x = 0; x < w; x++) {
				const n = noise2(x * 0.012, y * 0.17, seed + 40) * 0.7 + noise2(x * 0.05, y * 0.33, seed + 41) * 0.3;
				const a = (n - 0.56) * 3 * env;
				if (a > 0) this.tint(x, y, c, dq(a * 0.5, x, y));
			}
		}
	}

	private ground(top: number): void {
		const { sky, w, h, seed, spec } = this;
		const tones = spec.snowy
			? [this.snowShade, this.snow, this.snowHi]
			: [mix(sky.ground, this.near, 0.45), sky.ground, mix(sky.ground, sky.glow, 0.3)];
		for (let y = top; y < h; y++) {
			const k = f((y - top) / (h - top));
			for (let x = 0; x < w; x++) {
				const n = noise2(x * 0.03, y * 0.09, seed + 9) * 0.7 + noise2(x * 0.11, y * 0.25, seed + 10) * 0.3;
				let c = ramp(tones, 0.5 + (n - 0.5) * 1.7, x, y);
				c = mix(c, this.haze, dq(f(f(f(1 - k) * f(1 - k)) * f(0.4)), x, y));
				const sparkle = hash(x, y, seed + 11);
				if (spec.snowy && sparkle > f(0.996)) c = mix(c, WHITE, 0.7);
				if (!spec.snowy && sparkle > f(0.985))
					c = mix(c, sparkle > f(0.995) && !sky.night ? BLOSSOM[3] : this.near, 0.4);
				this.put(x, y, c);
			}
		}
	}

	/**
	 * A fir tree: overlapping, drooping tiers that widen downwards, snow (or rim light) along their upper
	 * side, a darker side away from the light and a shadow line under each tier. `hz` fades it into the haze.
	 */
	private pine(cx: number, baseY: number, height: number, hz: number): void {
		const { sky, w, h, seed, near, haze, lightDir, pixels } = this;
		const snowy = this.spec.snowy;
		const body = mix(near, haze, hz);
		const dark = mix(mix(near, INK, 0.32), haze, hz);
		const lit = mix(mix(near, sky.glow, sky.night ? 0.22 : 0.42), haze, hz);
		const capLit = mix(this.snowHi, haze, f(hz) * f(0.6));
		const capShade = mix(mix(this.snowShade, near, 0.22), haze, hz);
		const bark = mix(mix(0x3e2a3a, near, 0.45), haze, hz);
		// pixels is unused until the first render(), so it doubles as a "painted by this tree" mask.
		const id = ++this.stamp;
		const trunk = Math.max(2, idiv(height, 10));
		const crown = height - trunk;
		const tiers = clamp(idiv(crown, 11), 2, 8);
		const th = f(crown / f(f(tiers * f(0.5)) + f(0.6)));
		const maxHalf = f(height * f(f(0.27) + f(hash(cx, height, seed + 70) * f(0.08))));
		const top = baseY - height;
		const tw = Math.max(1, idiv(height, 40));
		this.rect(cx - idiv(tw, 2), baseY - trunk - Math.min(4, idiv(height, 8)), tw, trunk + Math.min(4, idiv(height, 8)), bark);
		for (let i = tiers - 1; i >= 0; i--) {
			// Each tier is a chevron: snowy upper slope from the stem out to thin, drooping branch tips.
			const ty = top + 2 + fi(f(i * th) * f(0.5));
			const grow = f(f(0.18) + f(f(f(0.82) * f(i + 1)) / tiers));
			const hl = f(f(maxHalf * grow) * f(f(0.85) + f(hash(cx, i, seed + 71) * f(0.3))));
			const hr = f(f(maxHalf * grow) * f(f(0.85) + f(hash(cx, i, seed + 76) * f(0.3))));
			for (let dx = -(hl | 0); dx <= (hr | 0); dx++) {
				const x = cx + dx;
				const hw = dx < 0 ? hl : hr;
				const u = f(Math.abs(dx) / f(hw + f(0.5)));
				const bump = u > f(0.3) && hash(x, ty, seed + 73) < f(0.2) ? 1 : 0;
				const finger = height >= 24 && u > 0.5 && (Math.abs(dx) + i) % 3 === 0 ? 2 : 0;
				const y0 = ty + fi(th * f(f(f(0.3) * u) + f(f(f(0.7) * u) * u))) + bump;
				const y1 = Math.max(y0, ty + fi(th * f(f(0.5) + f(f(0.55) * u))) - finger);
				const side = f((dx * lightDir) / hw);
				const cap = !snowy
					? 0
					: height < 16 || hz > f(0.6)
						? 1
						: 1 + (u < f(0.8) ? 1 : 0) + (hash(x, i, seed + 75) > 0.5 ? 1 : 0) + (height > 80 && u < 0.5 ? 1 : 0);
				for (let y = y0; y <= y1; y++) {
					const fromTop = y - y0;
					const under = f(fromTop / Math.max(1, y1 - y0));
					const c =
						fromTop < cap
							? side > f(-0.35) ? capLit : capShade
							: !snowy && fromTop === 0 && side > f(-0.2)
								? lit
								: side > f(0.45) && fromTop < 3
									? lit
									: height >= 20 && under > f(0.6) && u < f(0.85)
										? dark
										: side < f(-0.2) && (height < 30 || bayer(x, y) < f(-side * f(1.3)))
											? dark
											: snowy && fromTop < cap + 3 && hash(x, y, seed + 74) < f(0.14)
												? side > f(-0.35) ? capLit : capShade
												: hash(x, y, seed + 78) < f(0.07)
													? dark
													: body;
					this.put(x, y, c);
					if (x >= 0 && x < w && y >= 0 && y < h) pixels[y * w + x] = id;
				}
				// Shadow line where this tier overhangs the one below it.
				const sy = y1 + 1;
				if (u < f(0.9) && x >= 0 && x < w && sy >= 0 && sy < h && pixels[sy * w + x] === id) this.put(x, sy, dark);
			}
		}
		this.put(cx, top, snowy ? capLit : lit);
		this.put(cx, top + 1, snowy ? capLit : body);
		if (hz < 0.5 && snowy) {
			const sw = f(maxHalf * f(0.9));
			for (let dy = 0; dy <= 2; dy++)
				for (let dx = -(sw | 0); dx <= (sw | 0); dx++) {
					const x = cx + dx - lightDir * fi(sw / 3);
					const y = baseY - 1 + dy;
					const a = f(f(0.55) * f(f(1 - f(Math.abs(dx) / sw)) - f(dy / 3)));
					if (a > 0 && x >= 0 && x < w && y >= 0 && y < h && pixels[y * w + x] !== id)
						this.tint(x, y, this.snowShade, dq(a, x, y));
				}
		}
	}

	private pineRow(baseY: number, minH: number, varH: number, gap: number, varGap: number, hz: number, salt: number, clearX: number, clearW: number): void {
		const seed = this.seed;
		let x = -4 + fi(hash(salt, 0, seed) * gap);
		while (x < this.w + 4) {
			const ht = minH + fi(varH * (0.45 * hash(x, salt, seed) + 0.55 * fbm(x * 0.03, seed + salt)));
			if (Math.abs(x - clearX) > clearW + idiv(ht, 4)) this.pine(x, baseY + fi(hash(x, salt + 1, seed) * 3), ht, hz);
			x += gap + fi(hash(x, salt + 2, seed) * varGap);
		}
	}

	private ridge(x: number, baseY: number, amp: number, freq: number, s: number): number {
		return baseY - fi(fbm(x * freq, this.seed + s) * amp);
	}

	/** Rolling hill with a lit crest that fades down its face. */
	private hill(baseY: number, amp: number, freq: number, c: number, s: number): void {
		const { sky, w, h, lightDir } = this;
		const snowy = this.spec.snowy;
		const body = snowy ? mix(this.snow, c, 0.35) : c;
		const crest = snowy ? mix(this.snowHi, c, 0.2) : mix(c, sky.glow, 0.3);
		const tones = [mix(body, INK, 0.12), body, crest];
		for (let x = 0; x < w; x++) {
			const top = this.ridge(x, baseY, amp, freq, s);
			const face = f(f((this.ridge(x - 3, baseY, amp, freq, s) - this.ridge(x + 3, baseY, amp, freq, s)) * -lightDir) * f(0.06));
			for (let y = top; y < h; y++) {
				const d = y - top;
				this.put(x, y, ramp(tones, f(0.45) + face + f(Math.max(0, f(1 - f(d / 5))) * 0.5) - Math.min(f(d / 40), f(0.25)), x, y));
			}
		}
	}

	/** Ridged peaks with sunlit faces, a dithered snowline and mist pooling in the valleys. */
	private mountains(baseY: number, amp: number, freq: number, s: number, hz: number): void {
		const { sky, w, seed, haze, lightDir } = this;
		const rock = mix(this.near, haze, hz);
		const litRock = mix(rock, sky.glow, 0.24);
		const cap = mix(this.snowHi, haze, f(hz) * f(0.5));
		const capShade = mix(this.snowShade, rock, 0.45);
		const peak = (x: number) => {
			const r = 1 - Math.abs(noise2(x * freq, 0, seed + s) * 2 - 1);
			return baseY - amp * (0.2 + 0.8 * r * r) - fbm(x * freq * 4, seed + s + 1) * amp * 0.2;
		};
		const rocks = [mix(rock, INK, 0.2), rock, litRock];
		const snows = [mix(capShade, INK, 0.1), capShade, cap];
		for (let x = 0; x < w; x++) {
			const py = fi(peak(x));
			const face = clamp((peak(x + 4) - peak(x - 4)) * lightDir * 0.12, -0.5, 0.5);
			for (let y = py; y < baseY; y++) {
				const alt = (baseY - y) / amp;
				const snowLine = 0.68 + (noise2(x * 0.15, y * 0.25, seed + s + 2) - 0.5) * 0.3;
				// Faces turned to the light stay bright near the ridge; erosion streaks run down the slopes.
				let tone = 0.5 + face * Math.max(0.3, 1 - (y - py) / 16);
				if (noise2(x * 0.4, y * 0.07, seed + s + 3) > 0.68) tone -= 0.3;
				let c = ramp(alt > snowLine ? snows : rocks, tone, x, y);
				if (y === py) c = mix(c, sky.glow, 0.3);
				this.put(x, y, mix(c, haze, dq((0.4 - alt) * 1.2, x, y)));
			}
		}
	}

	private forest(): void {
		const { w, h, horizon, portrait } = this;
		this.ground(horizon);
		const railX = this.spec.foreground === 'Rails' ? fi(w * f(0.68)) : -999;
		const cabinX = fi(w * f(portrait ? 0.5 : 0.43));
		this.pineRow(horizon + 1, 6, 10, 2, 3, 0.74, 1, -999, 0);
		this.mist(horizon, 4, 0.35);
		this.pineRow(horizon + 3, 12, 18, 4, 5, 0.5, 2, railX, 4);
		this.mist(horizon + 4, 4, 0.25);
		this.house(cabinX, horizon + 10, 0.2);
		this.pineRow(horizon + 8, 22, 30, 9, 12, 0.22, 3, cabinX + 7, 12);
		if (portrait) {
			this.pine(fi(w * f(0.93)), h - 58, 80, 0.1);
			this.pine(fi(w * f(0.3)), h - 46, 96, 0.06);
			this.pine(fi(w * f(0.08)), h - 12, 160, 0);
		} else {
			this.pine(fi(w * f(0.86)), h - 36, 68, 0.1);
			this.pine(fi(w * f(0.17)), h - 24, 94, 0.06);
			this.pine(fi(w * f(0.95)), h - 12, 112, 0);
			this.pine(fi(w * f(0.05)), h - 6, 134, 0);
		}
	}

	private lake(): void {
		const { sky, w, h, seed, portrait, base, near } = this;
		const top = this.horizon;
		this.mountains(top - 2, portrait ? 72 : 60, 0.012, 4, 0.6);
		this.mountains(top + 1, 30, 0.022, 5, 0.36);
		this.mist(top - 2, 5, 0.3);
		const cabinX = fi(w * f(portrait ? 0.18 : 0.24));
		let x = -2;
		while (x < w + 2) {
			if (x < cabinX - 4 || x > cabinX + 18) this.pine(x, top + 1, 7 + fi(hash(x, 6, seed) * 12), 0.32);
			x += 2 + fi(hash(x, 7, seed) * 4);
		}
		this.house(cabinX, top + 1, 0.15);
		// The water mirrors everything above the shoreline, broken up by ripples.
		const tint = mix(sky.bands[1], near, 0.4);
		for (let y = top + 1; y < h; y++) {
			const k = f((y - top) / (h - top));
			const wob = fi((noise2(y * 0.6, 0, seed + 6) - 0.5) * (2 + k * 5));
			const sy = clamp(2 * top - y + 1, 0, h - 1);
			for (let xx = 0; xx < w; xx++) {
				let c = mix(base[sy * w + clamp(xx + wob, 0, w - 1)], tint, f(0.25) + f(k * f(0.4)));
				const rip = noise2(xx * 0.05, y * 0.7, seed + 8);
				if (rip > 0.7) c = mix(c, sky.bands[4], 0.2);
				else if (rip < 0.28) c = mix(c, INK, 0.12);
				this.put(xx, y, c, WATER);
			}
		}
	}

	private city(): void {
		const { sky, w, h, seed, near, base } = this;
		const street = h - 26;
		const tall = this.portrait ? f(1.7) : 1;
		const farC = mix(near, this.haze, 0.5);
		let x = -2;
		while (x < w) {
			const bw = 6 + fi(hash(x, 7, seed) * 11);
			const top = street - 34 - fi((fbm(x * 0.02, seed + 7) * 56 + hash(x, 8, seed) * 18) * tall);
			this.building(x, top, bw, street, farC, true);
			x += bw;
		}
		// City glow rising behind the nearer blocks.
		for (let y = street - 40; y < street; y++)
			for (let xx = 0; xx < w; xx++) this.tint(xx, y, sky.bands[5], dq(f(f((y - street + 40) / 40) * f(0.4)), xx, y));
		x = -4;
		while (x < w) {
			const bw = 12 + fi(hash(x, 10, seed) * 16);
			const top = street - 20 - fi(f(f(hash(x, 11, seed) * h) * f(0.3)) * tall);
			this.building(x, top, bw, street, near, false);
			if (hash(x, 12, seed) < f(0.4))
				this.neonSign(x + bw - 4, top + 5, 8 + fi(hash(x, 13, seed) * 8), NEON[fi(hash(x, 14, seed) * NEON.length)]);
			x += bw + fi(hash(x, 16, seed) * 6);
		}
		let lx = 10 + fi(hash(2, 17, seed) * 30);
		while (lx < w) {
			this.lamp(lx, street);
			lx += 48 + fi(hash(lx, 18, seed) * 30);
		}
		// Wet street mirrors what stands on it.
		const road = mix(near, INK, 0.3);
		for (let y = street; y < h; y++)
			for (let xx = 0; xx < w; xx++) {
				const src = base[Math.max(street - 1 - (y - street) * 2, 0) * w + xx];
				this.put(xx, y, bayer(xx, y) < 0.6 ? mix(road, src, 0.45) : road);
			}
		for (let xx = 0; xx < w; xx++) this.put(xx, street, mix(road, sky.accent, 0.2));
	}

	private building(x: number, top: number, bw: number, bottom: number, c: number, far: boolean): void {
		const { sky, seed, litP } = this;
		const r = hash(x, top, seed + 80);
		const rim = mix(c, sky.accent, far ? 0.12 : 0.25);
		this.rect(x, top, bw, bottom - top, c);
		const ex = this.lightDir > 0 ? x + bw - 1 : x;
		for (let y = top; y < bottom; y++) this.put(ex, y, rim);
		if (r < f(0.2)) this.rect(x - 1, top - 1, bw + 2, 1, rim);
		else if (r < f(0.4) && bw > 9) {
			this.rect(x + 2, top - 6, bw - 4, 6, c);
			this.rect(x + 2, top - 6, bw - 4, 1, rim);
		} else if (r < f(0.52) && !far && bw > 8) this.waterTower(x + idiv(bw, 2) - 2, top, c, rim);
		else if (r < f(0.66)) {
			this.line(x + idiv(bw, 2), top - 9, x + idiv(bw, 2), top - 1, c);
			this.beacon(x + idiv(bw, 2), top - 10);
		} else if (!far) {
			this.rect(x + 2, top - 2, 3, 2, c);
			if (bw > 10) this.rect(x + bw - 5, top - 3, 2, 3, c);
		}
		if (far) {
			for (let wy = top + 3; wy < bottom - 1; wy += 3)
				for (let wx = x + 1; wx < x + bw - 1; wx += 2) {
					const k = hash(wx, wy, seed + 1);
					if (k < f(f(litP) * f(0.5))) this.put(wx, wy, mix(WINDOWS[fi(k * 97) % WINDOWS.length], c, 0.45));
				}
			return;
		}
		const dim = mix(c, sky.bands[2], 0.3);
		for (let wy = top + 3; wy < bottom - 7; wy += 4) {
			// Whole floors tend to be awake together, like real buildings at night.
			const awake = hash(x, wy, seed + 81) < f(f(litP) * f(1.1));
			for (let wx = x + 2; wx < x + bw - 3; wx += 4) {
				const k = hash(wx, wy, seed);
				const on = k < (awake ? f(0.75) : f(f(litP) * f(0.5)));
				const wc = on ? WINDOWS[fi(k * 97) % WINDOWS.length] : dim;
				this.put(wx, wy, wc);
				this.put(wx + 1, wy, wc);
				this.put(wx, wy + 1, on ? mix(wc, c, 0.3) : wc);
				this.put(wx + 1, wy + 1, on ? mix(wc, c, 0.3) : wc);
			}
		}
		// Shopfront with an awning at street level.
		if (r > f(0.45) && bw > 10) {
			const awning = mix(NEON[fi(r * 31) % NEON.length], c, 0.35);
			for (let xx = x + 1; xx < x + bw - 1; xx++) this.put(xx, bottom - 6, xx % 2 === 0 ? awning : mix(awning, WHITE, 0.3));
			const glass = this.lightsOn ? mix(WINDOW_LIGHT, c, 0.4) : dim;
			for (let xx = x + 2; xx < x + bw - 2; xx++)
				for (let yy = bottom - 5; yy < bottom - 1; yy++)
					this.put(xx, yy, (xx - x) % 4 === 1 ? c : yy === bottom - 5 ? mix(glass, WHITE, 0.15) : glass);
		}
	}

	private waterTower(x: number, roof: number, c: number, rim: number): void {
		this.rect(x, roof - 3, 1, 3, c);
		this.rect(x + 4, roof - 3, 1, 3, c);
		this.rect(x, roof - 4, 5, 1, c);
		this.rect(x, roof - 9, 5, 5, c);
		for (let y = roof - 9; y < roof - 4; y++) this.put(this.lightDir > 0 ? x + 4 : x, y, rim);
		this.rect(x, roof - 7, 5, 1, mix(c, INK, 0.3));
		this.rect(x - 1, roof - 10, 7, 1, rim);
		this.rect(x + 1, roof - 11, 3, 1, c);
		this.put(x + 2, roof - 12, c);
	}

	private beacon(x: number, y: number): void {
		this.put(x, y, this.beaconDim);
		if (this.beaconCount < this.beacons.length && x >= 0 && x < this.w && y >= 0 && y < this.h)
			this.beacons[this.beaconCount++] = y * this.w + x;
	}

	private lamp(x: number, gy: number): void {
		const pole = mix(this.near, INK, 0.4);
		this.halo(x + 2, gy - 13, 5, WINDOW_LIGHT, 0.6);
		this.halo(x + 2, gy, 6, WINDOW_LIGHT, 0.35);
		this.rect(x, gy - 15, 1, 15, pole);
		this.rect(x, gy - 15, 3, 1, pole);
		this.put(x + 2, gy - 14, mix(WINDOW_LIGHT, WHITE, 0.4));
	}

	private neonSign(x: number, y: number, len: number, c: number): void {
		for (let yy = y - 2; yy <= y + len + 1; yy++) for (let xx = x - 2; xx <= x + 3; xx++) this.tint(xx, yy, c, 0.18);
		for (let yy = y; yy < y + len; yy++)
			for (let xx = x; xx <= x + 1; xx++) this.put(xx, yy, (yy - y) % 3 === 2 ? mix(c, this.near, 0.6) : c);
	}

	private blossomTree(cx: number, baseY: number, r: number, hz: number): void {
		const { sky, seed, haze, lightDir } = this;
		const bark = mix(mix(0x5a3c4e, this.near, 0.25), haze, hz);
		const barkLit = mix(bark, sky.glow, 0.3);
		const crownY = baseY - r - idiv(r, 2);
		const tw = Math.max(1, idiv(r, 6));
		for (let y = crownY; y < baseY; y++) {
			const k = f((y - crownY) / (baseY - crownY));
			const wd = tw + fi(f(f(f(k * k) * tw) * f(1.5)));
			const off = Math.round(f(f(f(Math.sin(f(f(k * f(1.8)) + cx))) * r) * f(0.06))) - idiv(wd, 2);
			for (let dx = 0; dx < wd; dx++)
				this.put(cx + off + dx, y, wd > 1 && dx === (lightDir > 0 ? wd - 1 : 0) ? barkLit : bark);
		}
		for (let k = 0; k < 4; k++) {
			const a = -PI / 2 + (k - 1.5) * 0.6 + (hash(cx, k + 30, seed) - 0.5) * 0.3;
			this.line(cx, crownY + idiv(r, 3), cx + ((Math.cos(a) * r * 0.9) | 0), crownY + idiv(r, 3) + ((Math.sin(a) * r * 0.7) | 0), bark);
		}
		// A wide dome of overlapping blossom puffs, painted from the rim inwards so the sunlit top sits in front.
		const n = 9 + idiv(r, 2);
		for (let m = 0; m < n; m++) {
			const k = m % 2 === 0 ? idiv(m, 2) : n - 1 - idiv(m, 2);
			const u = f((k + hash(cx, k, seed)) / n);
			const dome = f(Math.sin(u * PI));
			const px = cx + fi(f(f(u - f(0.5)) * r) * f(2.1));
			const py = crownY + idiv(r, 4) - fi(f(f(dome * r) * f(0.75)) + f(f(hash(cx, k + 10, seed) * r) * f(0.3)));
			const pr = Math.max(2, fi(r * f(f(f(0.36) + f(f(0.22) * dome)) + f(hash(cx, k + 20, seed) * f(0.15)))));
			this.puff(px, py, pr, hz, f(f(dome - f(0.5)) * f(0.25)));
		}
		if (hz < f(0.3)) {
			const rx = f(r * f(1.3));
			for (let dy = 0; dy <= 3; dy++)
				for (let dx = -(rx | 0); dx <= (rx | 0); dx++) {
					if (hash(cx + dx, baseY + dy, seed + 5) < f(f(0.22) * f(1 - f(Math.abs(dx) / rx))))
						this.put(cx + dx, baseY + dy - 1, BLOSSOM[2 + (dx & 1)]);
				}
		}
	}

	private puff(cx: number, cy: number, r: number, hz: number, bias: number): void {
		const { seed, lightDir } = this;
		const night = this.sky.night ? 0.45 : 0;
		for (let dy = -r; dy <= r; dy++)
			for (let dx = -r; dx <= r; dx++) {
				const x = cx + dx;
				const y = cy + dy;
				if (dx * dx + dy * dy > f(r * r * f(f(0.7) + f(f(0.45) * hash(x, y, seed + 3))))) continue;
				const tone = f(f(f(f(f(0.6) + bias) - f(f(dy * f(0.32)) / r)) + f(f(dx * lightDir * f(0.12)) / r)) + f(f(hash(x, y, seed + 4) - f(0.5)) * f(0.18)));
				this.put(x, y, mix(mix(ramp(BLOSSOM, tone, x, y), this.near, night), this.haze, hz));
			}
	}

	private blossoms(): void {
		const { w, h, seed, portrait, near, haze } = this;
		const g = this.horizon + 2;
		this.hill(g - 8, 26, 0.011, mix(near, haze, 0.72), 11);
		// Pink haze of distant groves along the second ridge.
		let x = 0;
		while (x < w) {
			if (fbm(x * 0.04, seed + 13) > 0.5) this.puff(x, this.ridge(x, g, 12, 0.02, 12) - 1, 2 + fi(hash(x, 14, seed) * 2), 0.55, 0);
			x += 3;
		}
		this.hill(g, 12, 0.02, mix(near, haze, 0.48), 12);
		this.ground(g + 6);
		x = fi(hash(0, 17, seed) * 20);
		while (x < w) {
			this.blossomTree(x, g + 8 + fi(hash(x, 18, seed) * 6), 7 + fi(hash(x, 19, seed) * 5), 0.32);
			x += 26 + fi(hash(x, 20, seed) * 22);
		}
		x = fi(hash(1, 17, seed) * 30) + (portrait ? 50 : 30);
		while (x < w - 20) {
			this.blossomTree(x, g + idiv(h - g, 2) + fi(hash(x, 25, seed) * 8), 10 + fi(hash(x, 26, seed) * 5), 0.12);
			x += (portrait ? 34 : 50) + fi(hash(x, 27, seed) * 40);
		}
		if (!portrait) this.blossomTree(fi(w * f(0.9)), h - 12, 22, 0);
		this.blossomTree(fi(w * f(0.1)), h - 8, portrait ? 32 : 28, 0);
	}

	private hills(): void {
		const { w, h, horizon, seed, near, haze } = this;
		const step = idiv(h - horizon, 4) + 6;
		for (let k = 0; k <= 3; k++) {
			const hz = f(f(0.72) - f(k * f(0.24)));
			const baseY = horizon - 14 + k * step;
			const freq = f(f(0.01) + f(k * f(0.004)));
			this.hill(baseY, 18, freq, mix(near, haze, hz), 20 + k);
			let x = fi(hash(k, 21, seed) * 9);
			while (k >= 1 && k <= 2 && x < w) {
				if (fbm(x * 0.05, seed + 22 + k) > 0.5) {
					this.pine(x, this.ridge(x, baseY, 18, freq, 20 + k) + 3 + fi(hash(x, 23, seed) * 3), 6 + k * 3 + fi(hash(x, 24, seed) * 6), hz);
				}
				x += 3 + fi(hash(x, 25, seed) * 3);
			}
			if (k === 3)
				for (const hx of [fi(w * f(0.22)), fi(w * f(0.64))]) this.house(hx, this.ridge(hx + 7, baseY, 18, freq, 23) + 5, 0);
			if (k < 3) this.mist(baseY + idiv(step, 2), 3, 0.18);
		}
	}

	/** Little cottage: lit gable front, receding side wall, chimney and warm windows after dark. */
	private house(x: number, gy: number, hz: number): void {
		const { sky, w, h, haze, near, lightsOn } = this;
		const snowy = this.spec.snowy;
		const wall = mix(mix(0x8a6a78, near, 0.45), haze, hz);
		const front = mix(wall, sky.glow, 0.18);
		const side = mix(wall, INK, 0.25);
		const roof = snowy ? mix(this.snowHi, haze, f(hz) * f(0.5)) : mix(mix(0x7a3f4c, near, 0.35), haze, hz);
		const glass = lightsOn ? WINDOW_LIGHT : mix(wall, sky.bands[2], 0.5);
		if (lightsOn) this.halo(x + 6, gy - 4, sky.night ? 9 : 12, WINDOW_LIGHT, sky.night ? 0.28 : 0.4);
		this.rect(x, gy - 7, 8, 7, front);
		this.rect(x + 8, gy - 7, 6, 7, side);
		for (let r = 0; r <= 4; r++) {
			for (let dx = 3 - r; dx <= 4 + r; dx++) this.put(x + dx, gy - 12 + r, dx === 3 - r || dx === 4 + r ? roof : front);
			for (let dx = 5 + r; dx <= 14; dx++) this.put(x + dx, gy - 12 + r, r === 4 ? mix(roof, INK, 0.2) : roof);
		}
		this.rect(x + 10, gy - 15, 2, 4, side);
		if (snowy) this.rect(x + 10, gy - 15, 2, 1, roof);
		if (this.chimneyCount < this.chimneys.length && x + 10 >= 0 && x + 10 < w && gy - 16 >= 0 && gy - 16 < h)
			this.chimneys[this.chimneyCount++] = (gy - 16) * w + x + 10;
		this.rect(x + 1, gy - 5, 2, 2, glass);
		this.rect(x + 9, gy - 5, 3, 2, glass);
		this.rect(x + 5, gy - 4, 2, 4, mix(wall, INK, 0.45));
		if (lightsOn) for (let dy = 0; dy <= 1; dy++) for (let dx = 0; dx <= 4; dx++) this.tint(x + dx, gy + dy, WINDOW_LIGHT, 0.2);
	}

	// endregion
	// region foregrounds

	private rails(): void {
		const { sky, w, h, near, portrait } = this;
		const snowy = this.spec.snowy;
		const top = this.horizon + 3;
		const farX = f(w * f(0.68));
		const nearX = f(w * f(portrait ? 0.4 : 0.34));
		const gauge = f(Math.min(w, h) * f(0.3));
		const rail = mix(near, INK, 0.45);
		const sheen = mix(rail, sky.glow, 0.5);
		const tie = mix(rail, snowy ? this.snowShade : sky.ground, 0.7);
		const trough = snowy ? this.snowShade : mix(sky.ground, near, 0.35);
		let pl = -1;
		let pr = -1;
		let wasTie = false;
		for (let y = top; y < h; y++) {
			const t = f((y - top + 1) / (h - top));
			// sqrt bends the far end sideways, so the track curves into the trees.
			const cx = f(farX + f(f(nearX - farX) * f(Math.sqrt(t))));
			const half = f(1 + f(f(gauge * t) / 2));
			const l = Math.round(f(cx - half));
			const r = Math.round(f(cx + half));
			const bw = f(f(half * f(1.4)) + 1);
			for (let x = fi(cx - bw); x <= fi(cx + bw); x++) {
				const e = f(f(Math.abs(f(x - cx))) / bw);
				this.tint(x, y, trough, dq(f(f(0.6) * f(1 - f(e * e))), x, y));
			}
			const isTie = t > f(0.1) && f(f(f(3.2) / f(t + f(0.06))) % 1) < f(0.16);
			if (isTie) {
				const c = !wasTie && snowy && t > f(0.35) ? mix(tie, this.snow, 0.5) : tie;
				for (let x = fi(l - f(half * f(0.18))); x <= fi(r + f(half * f(0.18))); x++) this.put(x, y, c);
			}
			wasTie = isTie;
			const thick = 1 + fi(t * f(2.2));
			const a0 = Math.min(pl < 0 ? l : pl, l);
			const b0 = Math.min(pr < 0 ? r : pr, r);
			for (let x = a0; x <= Math.max(pl < 0 ? l : pl, l) + thick - 1; x++) this.put(x, y, thick > 1 && x === a0 ? sheen : rail);
			for (let x = b0; x <= Math.max(pr < 0 ? r : pr, r) + thick - 1; x++) this.put(x, y, thick > 1 && x === b0 ? sheen : rail);
			pl = l;
			pr = r;
		}
	}

	private shore(): void {
		const { sky, w, h, seed, near, lightDir } = this;
		const c = mix(near, INK, 0.3);
		const rim = this.spec.snowy ? this.snow : mix(c, sky.glow, 0.2);
		const reed = mix(c, sky.ground, 0.25);
		const cattail = mix(0x5a3a30, near, 0.4);
		for (let x = 0; x < w; x++) {
			const ry = h - 18 + fi(fbm(x * 0.035, seed + 30) * 12);
			for (let y = ry; y < h; y++) this.put(x, y, y < ry + 3 && bayer(x, y) < 0.5 ? mix(c, rim, 0.3) : c);
			this.put(x, ry, rim);
			if (hash(x, 31, seed) < f(0.3)) {
				const rh = 3 + fi(hash(x, 32, seed) * 12);
				const lean = hash(x, 34, seed) < 0.5 ? -1 : 1;
				for (let k = 1; k <= rh; k++) this.put(x + (k > idiv(rh * 2, 3) ? lean : 0), ry - k, reed);
				if (rh > 8 && hash(x, 33, seed) < f(0.45)) this.rect(x + lean, ry - rh - 2, 1, 3, cattail);
			}
		}
		for (let k = 0; k <= 1; k++) {
			const rx = fi(w * f(f(0.62) + f(k * f(0.25))));
			const ry = h - 14 + fi(fbm(rx * 0.035, seed + 30) * 12) - 4;
			const rw = 7 - k * 2;
			for (let dy = idiv(-rw, 2); dy <= idiv(rw, 2); dy++)
				for (let dx = -rw; dx <= rw; dx++) {
					if (f(f((dx * dx) / f(rw * rw)) + f((dy * dy) / f((rw * rw) / 4))) > 1) continue;
					this.put(rx + dx, ry + dy, dy < 0 && dx * lightDir > -1 ? mix(c, sky.glow, 0.22) : mix(c, INK, 0.1));
				}
		}
	}

	private rooftops(): void {
		const { sky, w, h, seed, near, lightsOn } = this;
		const c = mix(near, INK, 0.35);
		const ledge = mix(c, sky.accent, 0.3);
		let x = 0;
		let catX = -1;
		let catY = 0;
		while (x < w) {
			const rw = 22 + fi(hash(x, 40, seed) * 30);
			const top = h - 12 - fi(hash(x, 41, seed) * 16);
			this.rect(x, top, rw, h - top, c);
			for (let dx = 0; dx < rw; dx++) this.put(x + dx, top, ledge);
			if (hash(x, 42, seed) < 0.5) {
				this.rect(x + 4, top - 5, 7, 5, c);
				this.rect(x + 4, top - 5, 7, 1, ledge);
				if (lightsOn) this.rect(x + 6, top - 3, 3, 2, mix(WINDOW_LIGHT, c, 0.2));
			}
			if (hash(x, 43, seed) < f(0.35)) {
				const ax = x + rw - 5;
				this.line(ax, top - 14, ax, top, c);
				this.line(ax - 2, top - 11, ax + 2, top - 11, c);
				this.beacon(ax, top - 15);
			} else if (catX < 0 && x > f(w * f(0.55)) && rw > 26) {
				catX = x + rw - 12;
				catY = top;
			}
			x += rw;
		}
		if (catX >= 0) {
			const eye = mix(0xe8f27a, c, 0.15);
			for (let r = 0; r < CAT.length; r++)
				for (let k = 0; k < CAT[r].length; k++) {
					const ch = CAT[r][k];
					if (ch !== '.')
						this.put(catX + k, catY - CAT.length + r, ch === '2' && lightsOn ? eye : r === 0 || CAT[r - 1][k] === '.' ? ledge : c);
				}
		}
		// A string of fairy lights between two poles.
		const x1 = fi(w * f(0.08));
		const x2 = fi(w * f(0.62));
		const y1 = h - 36;
		const y2 = h - 42;
		this.line(x1, y1, x1, h, c);
		this.line(x2, y2, x2, h, c);
		for (let lx = x1; lx <= x2; lx++) {
			const u = f((lx - x1) / (x2 - x1));
			const ly = (f(y1 + f((y2 - y1) * u)) + 8 * Math.sin(PI * u)) | 0;
			this.put(lx, ly, c);
			if ((lx - x1) % 5 === 2) {
				const bulb = BULBS[idiv(lx - x1, 5) % BULBS.length];
				this.halo(lx, ly + 1, 3, bulb, 0.5);
				this.put(lx, ly + 1, bulb);
			}
		}
	}

	private window(): void {
		const { sky, w, h, near, portrait } = this;
		const t = 7;
		const wood = mix(0x4a3640, near, 0.3);
		const bevel = mix(wood, sky.glow, 0.28);
		const frame = (x: number, y: number) =>
			x < t ||
			x >= w - t ||
			y < t ||
			y >= h - t * 2 ||
			Math.abs(x - idiv(w, 2)) < idiv(t, 2) + 1 ||
			(portrait && Math.abs(y - idiv(h * 9, 20)) < idiv(t, 2) + 1);
		for (let y = 0; y < h; y++)
			for (let x = 0; x < w; x++) {
				if (frame(x, y)) this.put(x, y, !frame(x, y - 1) || !frame(x - 1, y) ? bevel : wood, FRONT);
			}
		// Potted plant on the sill.
		const sill = h - t * 2;
		const px = w - t - 22;
		this.rect(px, sill - 7, 9, 7, mix(0xc0705a, near, 0.3), FRONT);
		this.rect(px - 1, sill - 8, 11, 2, mix(0xd88a6e, near, 0.3), FRONT);
		const leaf = mix(0x6fa07a, near, 0.35);
		const leafDark = mix(0x3f6e55, near, 0.35);
		for (let k = 0; k <= 6; k++) {
			const ex = px + 4 + (k - 3) * 3;
			const ey = sill - 14 - (3 - Math.abs(k - 3)) * 2;
			for (let s = 0; s <= 6; s++)
				this.put(px + 4 + idiv((ex - px - 4) * s, 6), sill - 8 + idiv((ey - sill + 8) * s, 6), k % 2 === 0 ? leaf : leafDark, FRONT);
		}
	}

	// endregion
	// region animation

	private flock(type: Particle, n: number): Flock | null {
		if (!this.has(type)) return null;
		const rnd = this.rnd;
		const fill = (scale: number) => {
			const a = new Float32Array(n);
			for (let i = 0; i < n; i++) a[i] = rnd.nextFloat() * scale;
			return a;
		};
		return { n, x: fill(this.w), y: fill(this.h), v: fill(1), p: fill(f(6.2832)) };
	}

	private plot(x: number, y: number, c: number, a: number, skyOnly = false): void {
		if (x < 0 || x >= this.w || y < 0 || y >= this.h) return;
		const i = y * this.w + x;
		const d = this.depth[i];
		if (d === FRONT || (skyOnly && d !== SKY_D)) return;
		this.pixels[i] = mix(this.pixels[i], c, a);
	}

	private shimmer(t: number): void {
		const { sky, w, h, seed, depth } = this;
		const top = this.horizon;
		const glow = sky.body === 'None' ? sky.bands[4] : sky.glow;
		const cx = sky.body === 'None' ? idiv(w, 2) : this.bodyCx;
		for (let y = top + 2; y < h; y++) {
			const half = 3 + idiv((y - top) * 16, h - top);
			const phase = f(hash(y, 60, seed) * f(6.28));
			const speed = f(f(0.8) + f(hash(y, 61, seed) * f(1.6)));
			for (let x = cx - half; x <= cx + half; x++) {
				if (x < 0 || x >= w || depth[y * w + x] !== WATER) continue;
				const edge = f(1 - f(Math.abs(x - cx) / (half + 1)));
				if (Math.sin(x * 0.45 + phase + t * speed) > 0.6 - edge * 0.6) this.plot(x, y, glow, f(0.15) + f(f(0.5) * edge));
			}
		}
		for (let k = 0; k < idiv(w, 6); k++) {
			const step = (t * 1.5 + hash(k, 62, seed) * 10) | 0;
			const x = fi(hash(k, step, seed + 63) * w);
			const y = top + 2 + fi(hash(k, step, seed + 64) * (h - top - 2));
			if (y < h && depth[y * w + x] === WATER) {
				this.plot(x, y, sky.bands[5], 0.35);
				this.plot(x + 1, y, sky.bands[5], 0.35);
			}
		}
	}

	private drawStars(t: number): void {
		const { sky, w, seed, pixels, stars, starA } = this;
		const c = mix(WHITE, sky.accent, 0.15);
		for (let k = 0; k < this.starCount; k++) {
			const i = stars[k];
			const tw = f(0.5 + 0.5 * Math.sin(t * (1.2 + (k % 5) * 0.45) + hash(k, 50, seed) * 6.28));
			const a = starA[k];
			const sc = k % 6 === 0 ? 0xbfd4ff : k % 9 === 0 ? 0xffe3b0 : c;
			pixels[i] = mix(pixels[i], sc, f(a * f(f(0.35) + f(f(0.65) * tw))));
			if (k % 7 === 0 && tw > f(0.7) && a > 0.5) {
				const x = i % w;
				const y = idiv(i, w);
				const g = f(f(f(tw - f(0.7)) * f(1.8)) * a);
				this.plot(x - 1, y, sc, g, true);
				this.plot(x + 1, y, sc, g, true);
				this.plot(x, y - 1, sc, g, true);
				this.plot(x, y + 1, sc, g, true);
			}
		}
	}

	private blink(t: number): void {
		const { w, base, pixels } = this;
		for (let k = 0; k < this.beaconCount; k++) {
			const i = this.beacons[k];
			// Skip beacons that something nearer was painted over.
			if (base[i] !== this.beaconDim || Math.sin(t * 1.9 + k * 2.3) < 0.35) continue;
			pixels[i] = BEACON | OPAQUE;
			const x = i % w;
			const y = idiv(i, w);
			this.plot(x - 1, y, BEACON, 0.35, true);
			this.plot(x + 1, y, BEACON, 0.35, true);
			this.plot(x, y - 1, BEACON, 0.35, true);
		}
	}

	private smoke(t: number): void {
		const { w, lightDir } = this;
		const c = mix(this.sky.bands[3], WHITE, 0.45);
		for (let k = 0; k < this.chimneyCount; k++) {
			const cx = this.chimneys[k] % w;
			const cy = idiv(this.chimneys[k], w);
			for (let j = 0; j < 6; j++) {
				const ph = (t * 0.16 + j / 6.0 + k * 0.37) % 1.0;
				const x = (cx + ph * 8 * -lightDir + Math.sin(ph * 7 + j + k) * 1.2) | 0;
				const y = (cy - ph * 18) | 0;
				const a = f(0.5 * (1 - ph));
				this.plot(x, y, c, a);
				if (ph > 0.3) this.plot(x + 1, y, c, f(a * f(0.7)));
			}
		}
	}

	private aimShootingStar(): void {
		const rnd = this.rnd;
		const dir = rnd.nextBoolean() ? 1 : -1;
		const angle = f(f(0.3) + f(rnd.nextFloat() * f(0.35)));
		this.shootX = f(this.w * f(f(0.2) + f(rnd.nextFloat() * f(0.6))));
		this.shootY = f(f(this.h * f(0.04)) + f(f(rnd.nextFloat() * this.horizon) * f(0.3)));
		this.shootDx = f(dir * f(Math.cos(angle)));
		this.shootDy = f(Math.sin(angle));
	}

	private shootingStar(t: number): void {
		const e = t - this.shootAt;
		if (e > 0.9) {
			this.shootAt = t + 8 + this.rnd.nextDouble() * 17;
			this.aimShootingStar();
			return;
		}
		if (e < 0) return;
		const fade = f(Math.sin((PI * e) / 0.9));
		const hx = f(this.shootX + f(f(this.shootDx * 140) * f(e)));
		const hy = f(this.shootY + f(f(this.shootDy * 140) * f(e)));
		for (let k = 0; k < 14; k++)
			this.plot(fi(hx - f(this.shootDx * k)), fi(hy - f(this.shootDy * k)), WHITE, f(fade * f(1 - f(k / 14))), true);
	}

	private drawClouds(t: number): void {
		const { w, h, depth, pixels } = this;
		for (let k = 0; k < this.cloudN; k++) {
			const cw = this.cloudW[k];
			const ox = wrap(this.cloudX[k] + this.cloudV[k] * t, w + cw) - cw;
			const sprite = this.cloudPix[k];
			for (let y = 0; y < this.cloudH[k]; y++)
				for (let x = 0; x < cw; x++) {
					const tone = sprite[y * cw + x];
					const px = ox + x;
					const py = this.cloudY[k] + y;
					if (tone === 0 || px < 0 || px >= w || py < 0 || py >= h) continue;
					const i = py * w + px;
					if (depth[i] <= MOON) pixels[i] = this.cloudTones[tone];
				}
		}
	}

	/** 0 = empty, 1 = shade, 2 = body, 3 = lit top edge. */
	private cloudSprite(cw: number, ch: number): Uint8Array {
		const puffs = 2 + idiv(cw, 9);
		const rMax = f((ch - 2) / f(1.6));
		const pr: number[] = [];
		for (let i = 0; i < puffs; i++)
			pr.push(f(f(rMax * f(f(0.55) + f(f(0.45) * f(Math.sin(f(f(f(PI) * f(i + 0.5)) / puffs)))))) * f(f(0.85) + f(this.rnd.nextFloat() * f(0.15)))));
		const px = pr.map((_, i) => f(pr[0] + f(f(f(cw - f(pr[0] * 2)) * i) / (puffs - 1))));
		const inside = (x: number, y: number) =>
			y >= 0 &&
			y < ch - 1 &&
			pr.some((r, i) => {
				const dx = f(x - px[i]);
				const dy = f(y - f(ch - 2 - f(r * f(0.55))));
				return f(f(dx * dx) + f(dy * dy)) <= f(r * r);
			});
		const out = new Uint8Array(cw * ch);
		for (let i = 0; i < out.length; i++) {
			const x = i % cw;
			const y = idiv(i, cw);
			out[i] = !inside(x, y) ? 0 : !inside(x, y - 1) ? 3 : y >= ch - 2 || (y >= ch - 4 && bayer(x, y) < 0.5) ? 1 : 2;
		}
		return out;
	}

	private lightning(t: number): void {
		const { rnd, pixels, depth, bolt } = this;
		const e = t - this.boltAt;
		if (e > 0.6) {
			this.boltAt = t + 12 + rnd.nextDouble() * 20;
			let x = fi(this.w * f(f(0.15) + f(rnd.nextFloat() * f(0.7))));
			this.boltLen = fi(this.horizon * f(f(0.5) + f(rnd.nextFloat() * f(0.4))));
			for (let y = 0; y < this.boltLen; y++) {
				x += rnd.nextIn(-1, 2) + (rnd.nextFloat() < f(0.08) ? rnd.nextIn(-3, 4) : 0);
				bolt[y] = x;
			}
			return;
		}
		if (e < 0) return;
		const k = e < 0.07 ? 1 : e < 0.14 ? 0.25 : e < 0.22 ? 0.8 : f(0.8 * (1 - (e - 0.22) / 0.38));
		const flash = mix(WHITE, this.sky.bands[4], 0.3);
		const sky = f(f(k) * f(0.45));
		const land = f(f(k) * f(0.15));
		for (let i = 0; i < pixels.length; i++) pixels[i] = mix(pixels[i], flash, depth[i] === SKY_D ? sky : land);
		if (k > 0.5) for (let y = 0; y < this.boltLen; y++) this.plot(bolt[y], y, WHITE, 1, true);
	}

	/** Three depths of flakes: faint distant specks, single pixels and soft 2x2 flakes close by. */
	private snowfall(fl: Flock, t: number): void {
		const { sky, w, h } = this;
		const soft = mix(WHITE, sky.bands[0], 0.1);
		const pink = mix(sky.accent, WHITE, 0.25);
		for (let i = 0; i < fl.n; i++) {
			const v = fl.v[i];
			const layer = i % 3;
			const y = wrap(fl.y[i] + t * f(f(5 + layer * 7) + f(v * 5)), h + 4) - 2;
			const sway = Math.sin(t * (0.5 + f(v * f(0.6))) + fl.p[i]) * (1.5 + layer * 1.5);
			const x = wrap(fl.x[i] + t * (1.5 + layer) + sway, w);
			const c = i % 4 === 1 ? pink : soft;
			if (layer === 0) this.plot(x, y, c, 0.4);
			else if (layer === 1) this.plot(x, y, c, 0.8);
			else {
				this.plot(x, y, c, 0.95);
				this.plot(x + 1, y, c, 0.55);
				this.plot(x, y + 1, c, 0.55);
				this.plot(x + 1, y + 1, c, 0.3);
			}
		}
	}

	/** Petals tumble through four orientations as they drift on the breeze. */
	private petals(fl: Flock, t: number): void {
		const { w, h } = this;
		for (let i = 0; i < fl.n; i++) {
			const v = fl.v[i];
			const close = i % 3 === 0;
			const y = wrap(fl.y[i] + t * f(f(8 + f(v * 10)) + (close ? 6 : 0)), h + 4) - 2;
			const x = wrap(fl.x[i] + t * f(7 + f(v * 9)) + Math.sin(t * 1.3 + fl.p[i]) * 5, w);
			const c = BLOSSOM[2 + (i % 3)];
			const d = BLOSSOM[1];
			switch (((t * (2.5 + f(v * 3)) + fl.p[i]) % 4) | 0) {
				case 0:
					this.plot(x, y, c, 0.95);
					this.plot(x + 1, y, d, 0.85);
					if (close) this.plot(x - 1, y, c, 0.6);
					break;
				case 1:
					this.plot(x, y, c, 0.95);
					this.plot(x + 1, y + 1, d, 0.8);
					break;
				case 2:
					this.plot(x, y, d, 0.9);
					break;
				default:
					this.plot(x, y, c, 0.95);
					this.plot(x, y + 1, d, 0.85);
					if (close) this.plot(x, y - 1, c, 0.6);
			}
		}
	}

	/** Faint short streaks far away, long bright ones up close, and splashes where drops land. */
	private rain(fl: Flock, t: number): void {
		const { sky, w, h, seed, horizon, depth } = this;
		const farC = mix(sky.bands[3], WHITE, 0.2);
		const nearC = mix(sky.bands[4], WHITE, 0.5);
		for (let i = 0; i < fl.n; i++) {
			const v = fl.v[i];
			const close = i % 3 === 0;
			const speed = close ? f(230 + f(v * 60)) : f(120 + f(v * 50));
			const len = close ? 6 + fi(v * 3) : 2 + fi(v * 2);
			const c = close ? nearC : farC;
			const a = close ? f(0.6) : f(0.3);
			const y = wrap(fl.y[i] + t * speed, h + len);
			const x = wrap(fl.x[i] - t * speed * 0.22, w);
			for (let k = 0; k < len; k++) this.plot(x + idiv(k, 4), y - k, c, f(a * f(1 - f(f(k * f(0.6)) / len))));
		}
		const step = (t * 12) | 0;
		for (let k = 0; k < idiv(w, 8); k++) {
			const sx = fi(hash(k, step, seed + 90) * w);
			const sy = horizon + fi(hash(k, step, seed + 91) * (h - horizon));
			if (sy >= h || depth[sy * w + sx] <= MOON) continue;
			this.plot(sx - 1, sy - 1, nearC, 0.45);
			this.plot(sx + 1, sy - 1, nearC, 0.45);
			this.plot(sx, sy, nearC, 0.3);
		}
	}

	private fireflies(fl: Flock, t: number): void {
		const { h, horizon } = this;
		const c = this.sky.accent;
		for (let i = 0; i < fl.n; i++) {
			const v = fl.v[i];
			const p = fl.p[i];
			const b = f(Math.sin(t * (1.1 + v) + f(p * 3)));
			if (b <= 0) continue;
			const x = (fl.x[i] + Math.sin(t * 0.37 * (1 + v) + p) * 8) | 0;
			const y = (f(f(horizon - 24) + f(f(fl.y[i] / h) * (h - horizon + 16))) + Math.sin(t * 0.53 + f(p * f(1.7))) * 5) | 0;
			const a = f(b * b);
			this.plot(x, y, mix(c, WHITE, 0.3), a);
			this.plot(x - 1, y, c, f(a * f(0.4)));
			this.plot(x + 1, y, c, f(a * f(0.4)));
			this.plot(x, y - 1, c, f(a * f(0.4)));
			this.plot(x, y + 1, c, f(a * f(0.4)));
		}
	}

	// endregion
}
