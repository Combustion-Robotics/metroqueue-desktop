<script lang="ts">
	// A vibe's pixel-art scene (`vibescene.ts`), drawn at 180px on the short side and upscaled
	// without smoothing, as on the phone. The static layers are painted once per size; a frame only
	// adds the particles. Animates at ~30 fps, and only while it is on screen and the window is
	// shown (docs/UI-PERFORMANCE.md).
	import { SceneRenderer, type SceneSpec } from '$lib/vibescene';
	import { vibes, watchUiVisible } from '$lib/vibes.svelte';

	let { spec, animate = true, class: cls = '' }: { spec: SceneSpec; animate?: boolean; class?: string } = $props();

	const SHORT_SIDE = 180;
	// Sleeping between frames caps us at ~30 fps without waking on every vsync.
	const FRAME_GAP_MS = 28;

	let canvas: HTMLCanvasElement;
	let boxW = $state(0);
	let boxH = $state(0);
	let onScreen = $state(false);
	let pageVisible = $state(true);

	const renderer = $derived.by(() => {
		const short = Math.min(boxW, boxH);
		if (short <= 0) return null;
		const w = Math.round((boxW * SHORT_SIDE) / short);
		const h = Math.round((boxH * SHORT_SIDE) / short);
		return new SceneRenderer($state.snapshot(spec), w, h);
	});
	const running = $derived(animate && onScreen && pageVisible && vibes.uiVisible);

	$effect(() => {
		watchUiVisible();
		const io = new IntersectionObserver(([e]) => (onScreen = e.isIntersecting));
		io.observe(canvas);
		const vis = () => (pageVisible = document.visibilityState === 'visible');
		vis();
		document.addEventListener('visibilitychange', vis);
		return () => {
			io.disconnect();
			document.removeEventListener('visibilitychange', vis);
		};
	});

	$effect(() => {
		const r = renderer;
		if (!r) return;
		canvas.width = r.w;
		canvas.height = r.h;
		const ctx = canvas.getContext('2d');
		if (!ctx) return;
		const img = ctx.createImageData(r.w, r.h);
		const out = new Uint32Array(img.data.buffer);
		const publish = () => {
			const px = r.pixels;
			// 0xAARRGGBB to the RGBA bytes of a little-endian Uint32 (0xAABBGGRR).
			for (let i = 0; i < px.length; i++) {
				const p = px[i];
				out[i] = (p & 0xff00ff00) | ((p >>> 16) & 0xff) | ((p & 0xff) << 16);
			}
			ctx.putImageData(img, 0, 0);
		};
		publish();
		if (!running) return;
		let timer: ReturnType<typeof setTimeout>;
		let raf = 0;
		let last = performance.now();
		const frame = (now: number) => {
			// A long gap (a stall, a tab switch) skips ahead at most 0.1 s, so nothing jumps.
			r.time += Math.min((now - last) / 1000, 0.1);
			last = now;
			r.render();
			publish();
			timer = setTimeout(() => (raf = requestAnimationFrame(frame)), FRAME_GAP_MS);
		};
		timer = setTimeout(() => (raf = requestAnimationFrame(frame)), FRAME_GAP_MS);
		return () => {
			clearTimeout(timer);
			cancelAnimationFrame(raf);
		};
	});
</script>

<div class="overflow-hidden {cls}" bind:clientWidth={boxW} bind:clientHeight={boxH}>
	<canvas bind:this={canvas} class="block h-full w-full [image-rendering:pixelated]"></canvas>
</div>
