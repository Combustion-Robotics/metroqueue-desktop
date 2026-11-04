<script lang="ts">
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import { RemoveIcon, SmartPhone01Icon } from '@hugeicons/core-free-icons';
	import { Button } from '$lib/components/ui/button';
	import { Switch } from '$lib/components/ui/switch';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { toast } from '$lib/player.svelte';
	import { t } from '$lib/i18n.svelte';

	type Device = { id: string; name: string; paired_at_ms: number };

	const GROUP = 'mb-7 last:mb-1';
	const LABEL =
		'mb-2 px-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground';
	const CARD = 'divide-y divide-border/60 overflow-hidden rounded-xl border bg-card';

	let enabled = $state(false);
	let devices = $state<Device[]>([]);
	let forgetting = $state<Device | null>(null);
	let confirmOpen = $state(false);

	const refresh = () =>
		invoke<Device[]>('mq_paired_devices')
			.then((d) => (devices = d))
			.catch(() => {});

	onMount(() => {
		invoke<boolean>('mq_get_enabled')
			.then((v) => (enabled = v))
			.catch(() => {});
		refresh();
		const un = listen('mq://devices-changed', refresh);
		return () => un.then((f) => f());
	});

	async function setEnabled(on: boolean) {
		enabled = on;
		try {
			await invoke('mq_set_enabled', { enabled: on });
		} catch (e) {
			enabled = !on;
			toast.error(String(e));
		}
	}

	async function forget() {
		if (!forgetting) return;
		// bits-ui's Action doesn't close the dialog on its own.
		confirmOpen = false;
		try {
			await invoke('mq_forget_device', { id: forgetting.id });
			refresh();
		} catch (e) {
			toast.error(String(e));
		}
	}
</script>

<section class={GROUP}>
	<h3 class={LABEL}>{t('settings.sections.metroqueue')}</h3>
	<div data-settings-card class={CARD}>
		<div class="flex items-start justify-between gap-6 px-4 py-3.5">
			<div class="min-w-0">
				<span class="text-sm font-medium">{t('settings.metroqueue.allow')}</span>
				<p class="mt-1 max-w-prose text-xs leading-relaxed text-muted-foreground">
					{t('settings.metroqueue.allow_hint')}
				</p>
			</div>
			<Switch class="shrink-0" checked={enabled} onCheckedChange={setEnabled} />
		</div>
		<div class="px-4 py-3.5">
			<span class="text-sm font-medium">{t('settings.metroqueue.paired')}</span>
			<div class="mt-3">
				{#if !devices.length}
					<p class="text-xs leading-relaxed text-muted-foreground">
						{t('settings.metroqueue.empty')}
					</p>
				{:else}
					<div class="flex flex-col gap-2">
						{#each devices as d (d.id)}
							<div
								class="flex items-center gap-2.5 rounded-lg bg-muted/60 py-1.5 pr-1.5 pl-3"
							>
								<HugeiconsIcon
									icon={SmartPhone01Icon}
									class="h-4 w-4 shrink-0 text-muted-foreground"
								/>
								<div class="min-w-0 flex-1">
									<p class="truncate text-xs">{d.name}</p>
									<p class="truncate text-[11px] text-muted-foreground">
										{t('settings.metroqueue.paired_at', {
											date: new Date(d.paired_at_ms).toLocaleDateString()
										})}
									</p>
								</div>
								<Button
									variant="ghost"
									size="icon"
									class="h-7 w-7 shrink-0"
									aria-label={t('settings.metroqueue.forget', { name: d.name })}
									onclick={() => {
										forgetting = d;
										confirmOpen = true;
									}}
								>
									<HugeiconsIcon icon={RemoveIcon} class="h-3.5 w-3.5" />
								</Button>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		</div>
	</div>
</section>

<AlertDialog.Root bind:open={confirmOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Media class="text-muted-foreground">
				<HugeiconsIcon icon={SmartPhone01Icon} />
			</AlertDialog.Media>
			<AlertDialog.Title>
				{t('settings.metroqueue.forget_title', { name: forgetting?.name ?? '' })}
			</AlertDialog.Title>
			<AlertDialog.Description>{t('settings.metroqueue.forget_desc')}</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel>{t('common.cancel')}</AlertDialog.Cancel>
			<AlertDialog.Action onclick={forget}>{t('settings.metroqueue.forget_confirm')}</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
