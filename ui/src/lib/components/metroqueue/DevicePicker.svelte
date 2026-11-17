<script lang="ts">
	// The player bar's devices button: MetroQueue phones, TVs and other desktops on the network.
	// Pair with one, control it (or move what plays here over to it), take its queue back. Rust
	// (`metroqueue/remote.rs`) does the talking; this lists and asks.
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import {
		CastIcon,
		ComputerIcon,
		RemoveIcon,
		SmartPhone01Icon,
		Tv01Icon
	} from '@hugeicons/core-free-icons';
	import * as Popover from '$lib/components/ui/popover';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { playback, toast } from '$lib/player.svelte';
	import { remote, remoteError, type Receiver } from '$lib/remote.svelte';
	import { t } from '$lib/i18n.svelte';

	let open = $state(false);
	let receivers = $state<Receiver[]>([]);
	let busy = $state(false);
	/** The receiver showing its code field. */
	let pairing = $state<string | null>(null);
	let code = $state('');

	const refresh = () =>
		invoke<Receiver[]>('mq_remote_receivers')
			.then((r) => (receivers = r))
			.catch(() => {});

	$effect(() => {
		if (!open) return;
		invoke('mq_remote_discover', { open: true }).catch(() => {});
		refresh();
		const un = listen('mq://receivers-changed', refresh);
		return () => {
			un.then((f) => f());
			invoke('mq_remote_discover', { open: false }).catch(() => {});
			pairing = null;
		};
	});

	async function run(cmd: string, args: Record<string, unknown>, done?: () => void) {
		busy = true;
		try {
			await invoke(cmd, args);
			done?.();
		} catch (e) {
			toast.error(remoteError(String(e)));
		} finally {
			busy = false;
		}
	}

	const startPairing = (r: Receiver) =>
		run('mq_remote_pair_start', { id: r.id }, () => {
			pairing = r.id;
			code = '';
		});

	const finishPairing = (r: Receiver) =>
		run('mq_remote_pair_finish', { id: r.id, code }, () => {
			pairing = null;
			toast.success(t('devices.paired', { name: r.name }));
			refresh();
		});

	const connect = (r: Receiver, moveHere: boolean) =>
		run('mq_remote_connect', { id: r.id, moveHere }, () => (open = false));

	const disconnect = (continueHere: boolean) =>
		run('mq_remote_disconnect', { continueHere }, () => (open = false));

	const kindIcon = (kind: string) =>
		kind === 'phone' ? SmartPhone01Icon : kind === 'desktop' ? ComputerIcon : Tv01Icon;
</script>

<Popover.Root bind:open>
	<Popover.Trigger
		class="state-layer inline-flex size-8 cursor-pointer items-center justify-center rounded-full {remote.device
			? 'bg-secondary-container text-primary'
			: ''}"
		aria-label={remote.device ? t('devices.playing_on', { name: remote.device.name }) : t('devices.title')}
		title={remote.device ? t('devices.playing_on', { name: remote.device.name }) : t('devices.title')}
	>
		<HugeiconsIcon icon={CastIcon} class="h-5 w-5" />
	</Popover.Trigger>
	<Popover.Content side="top" align="end" sideOffset={8} class="w-80 gap-2 p-2">
		<p class="px-2 pt-1 text-label-lg text-on-surface-variant">
			{#if remote.status === 'reconnecting' && remote.device}
				{t('devices.reconnecting', { name: remote.device.name })}
			{:else if remote.device}
				{t('devices.playing_on', { name: remote.device.name })}
			{:else}
				{t('devices.title')}
			{/if}
		</p>
		{#if !receivers.length}
			<p class="px-2 pb-2 text-body-sm text-on-surface-variant">{t('devices.searching')}</p>
		{/if}
		{#each receivers as r (r.id)}
			{@const connected = remote.device?.id === r.id}
			<div class="flex flex-col gap-2 rounded-lg px-2 py-2 {connected ? 'bg-secondary-container/60' : ''}">
				<div class="flex items-center gap-2.5">
					<HugeiconsIcon
						icon={kindIcon(r.kind)}
						class="h-5 w-5 shrink-0 {connected ? 'text-primary' : 'text-on-surface-variant'}"
					/>
					<div class="min-w-0 flex-1">
						<p class="truncate text-body-md">{r.name}</p>
						<p class="truncate text-body-sm text-on-surface-variant">
							{connected
								? t('devices.connected')
								: !r.online
									? t('devices.offline')
									: r.paired
										? t('devices.ready')
										: t('devices.not_paired')}
						</p>
					</div>
					{#if r.paired}
						<Button
							variant="ghost"
							size="icon-sm"
							disabled={busy}
							aria-label={t('devices.forget', { name: r.name })}
							title={t('devices.forget', { name: r.name })}
							onclick={() => run('mq_remote_forget', { id: r.id }, refresh)}
						>
							<HugeiconsIcon icon={RemoveIcon} class="h-4 w-4" />
						</Button>
					{/if}
				</div>
				{#if connected}
					<div class="flex flex-wrap gap-2">
						<Button size="sm" disabled={busy} onclick={() => disconnect(true)}>
							{t('devices.continue_here')}
						</Button>
						<Button size="sm" variant="outline" disabled={busy} onclick={() => disconnect(false)}>
							{t('devices.disconnect')}
						</Button>
					</div>
				{:else if r.paired}
					<div class="flex flex-wrap gap-2">
						{#if playback.now}
							<Button size="sm" disabled={busy} onclick={() => connect(r, true)}>
								{t('devices.play_there', { name: r.name })}
							</Button>
						{/if}
						<Button size="sm" variant="outline" disabled={busy} onclick={() => connect(r, false)}>
							{t('devices.control')}
						</Button>
					</div>
				{:else if pairing === r.id}
					<form
						class="flex flex-col gap-2"
						onsubmit={(e) => {
							e.preventDefault();
							finishPairing(r);
						}}
					>
						<p class="text-body-sm text-on-surface-variant">
							{t('devices.pair_hint', { name: r.name })}
						</p>
						<div class="flex gap-2">
							<Input
								bind:value={code}
								inputmode="numeric"
								autocomplete="off"
								maxlength={6}
								placeholder="000000"
								aria-label={t('devices.code')}
								class="h-8 flex-1 font-mono tracking-[0.2em] tabular-nums"
							/>
							<Button size="sm" type="submit" disabled={busy || code.trim().length !== 6}>
								{t('devices.pair')}
							</Button>
						</div>
					</form>
				{:else if r.online}
					<div>
						<Button size="sm" variant="outline" disabled={busy} onclick={() => startPairing(r)}>
							{t('devices.pair')}
						</Button>
					</div>
				{/if}
			</div>
		{/each}
	</Popover.Content>
</Popover.Root>
