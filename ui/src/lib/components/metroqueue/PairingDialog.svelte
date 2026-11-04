<script lang="ts">
	// A MetroQueue phone asked to pair: show the code it has to type in. Rust decides the outcome
	// and emits `mq://pairing-done`; this only shows the code and can cancel it.
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { toast } from '$lib/player.svelte';
	import { formatClock } from '$lib/timecode';
	import { t } from '$lib/i18n.svelte';

	type PairingCode = { code: string; device_name: string; expires_at_ms: number };

	// Kept after closing so the dialog doesn't empty out during its close animation.
	let pending = $state<PairingCode | null>(null);
	let open = $state(false);
	let now = $state(Date.now());
	const left = $derived(pending ? Math.max(0, Math.ceil((pending.expires_at_ms - now) / 1000)) : 0);

	$effect(() => {
		if (!open) return;
		const id = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(id);
	});
	$effect(() => {
		if (open && left === 0) open = false;
	});

	function cancel() {
		open = false;
		invoke('mq_cancel_pairing').catch((e) => toast.error(String(e)));
	}

	onMount(() => {
		const uns = [
			listen<PairingCode>('mq://pairing-code', (e) => {
				now = Date.now();
				pending = e.payload;
				open = true;
			}),
			listen<{ device_name: string; success: boolean }>('mq://pairing-done', (e) => {
				const { device_name: name, success } = e.payload;
				// No failure toast once the dialog is gone: the user cancelled it or watched it expire.
				if (success) toast.success(t('toasts.mq_paired', { name }));
				else if (open) toast.error(t('toasts.mq_pairing_failed', { name }));
				open = false;
			})
		];
		return () => uns.forEach((un) => un.then((f) => f()));
	});
</script>

<Dialog.Root {open} onOpenChange={(o) => !o && cancel()}>
	<Dialog.Content class="sm:max-w-sm" showCloseButton={false}>
		{#if pending}
			<Dialog.Header class="items-center text-center">
				<Dialog.Title>{t('dialogs.pairing.title', { name: pending.device_name })}</Dialog.Title>
				<Dialog.Description>{t('dialogs.pairing.desc')}</Dialog.Description>
			</Dialog.Header>
			<div class="flex flex-col items-center gap-2">
				<span
					class="font-mono text-5xl font-semibold tracking-[0.2em] tabular-nums text-primary select-all"
					aria-live="polite"
				>
					{pending.code}
				</span>
				<span class="text-xs text-muted-foreground tabular-nums">
					{t('dialogs.pairing.expires', { time: formatClock(left) })}
				</span>
			</div>
			<Dialog.Footer class="sm:justify-center">
				<Button variant="outline" onclick={cancel}>{t('common.cancel')}</Button>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>
