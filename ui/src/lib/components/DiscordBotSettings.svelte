<script lang="ts">
	// The built-in Discord bot: a token, an on switch, and what the bot is doing right now. The
	// token is write-only from here: Rust never hands it back, so the field only ever says whether
	// one is saved.
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import { LinkSquare02Icon, Loading03Icon } from '@hugeicons/core-free-icons';
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Switch } from '$lib/components/ui/switch';
	import * as api from '$lib/api';
	import { toast } from '$lib/player.svelte';
	import { t } from '$lib/i18n.svelte';

	const GROUP = 'mb-7 last:mb-1';
	const LABEL = 'mb-2 px-1 text-label-lg text-primary';
	const CARD = 'flex flex-col gap-0.5 overflow-hidden rounded-xl';

	let status = $state<api.DiscordBotStatus | null>(null);
	let token = $state('');
	let tokenError = $state<string | null>(null);
	let saving = $state(false);

	onMount(() => {
		const un = api.onDiscordBotStatus((s) => (status = s));
		api.discordBotStatus()
			.then((s) => (status = s))
			.catch(() => {});
		return () => un.then((u) => u());
	});

	async function setToken(value: string) {
		saving = true;
		tokenError = null;
		try {
			await api.discordBotSetToken(value);
			token = '';
			status = await api.discordBotStatus();
		} catch (e) {
			tokenError = String(e);
		} finally {
			saving = false;
		}
	}

	async function setEnabled(on: boolean) {
		if (!status) return;
		const was = status.enabled;
		status.enabled = on;
		try {
			await api.discordBotSetEnabled(on);
		} catch (e) {
			if (status) status.enabled = was;
			toast.error(String(e));
		}
	}

	const statusText = $derived.by(() => {
		switch (status?.state) {
			case 'connecting':
				return t('settings.discord.bot_status_connecting');
			case 'online':
				return t('settings.discord.bot_status_online', { name: status.botName ?? '' });
			case 'error':
				return status.error || t('settings.discord.bot_status_error');
			default:
				return t('settings.discord.bot_status_off');
		}
	});
	const statusDot = $derived(
		status?.state === 'online'
			? 'bg-primary'
			: status?.state === 'error'
				? 'bg-destructive'
				: status?.state === 'connecting'
					? 'bg-primary/50 animate-pulse'
					: 'bg-muted-foreground/40'
	);
</script>

<section class={GROUP}>
	<h3 class={LABEL}>{t('settings.discord.bot_section')}</h3>
	<div data-settings-card class={CARD}>
		<div class="px-4 py-3.5">
			<div class="flex items-center justify-between gap-4">
				<span class="min-w-0 text-sm font-medium">{t('settings.discord.bot_enable')}</span>
				<Switch
					checked={status?.enabled ?? false}
					disabled={!status || (!status.hasToken && !status.enabled)}
					onCheckedChange={setEnabled}
					aria-label={t('settings.discord.bot_enable')}
				/>
			</div>
			<p class="mt-1.5 text-xs leading-relaxed text-muted-foreground">
				{t('settings.discord.bot_enable_hint')}
			</p>
			<div class="mt-3 flex items-center gap-2 text-xs" aria-live="polite">
				<span class="h-2 w-2 shrink-0 rounded-full {statusDot}"></span>
				<span class="min-w-0 {status?.state === 'error' ? 'text-destructive' : 'text-muted-foreground'}">
					{statusText}
				</span>
			</div>
			{#if status?.state === 'online'}
				{#if status.voice.length}
					<ul class="mt-2 flex flex-col gap-0.5 pl-4 text-xs text-muted-foreground">
						{#each status.voice as v, i (i)}
							<li class="truncate">{v.guild} · {v.channel}</li>
						{/each}
					</ul>
				{/if}
				{#if status.inviteUrl}
					<Button
						variant="secondary"
						size="sm"
						class="mt-3 gap-1.5"
						onclick={() => status?.inviteUrl && api.openExternal(status.inviteUrl)}
					>
						<HugeiconsIcon icon={LinkSquare02Icon} size={15} />
						{t('settings.discord.bot_invite')}
					</Button>
				{/if}
			{/if}
		</div>

		<div class="px-4 py-3.5">
			<span class="text-sm font-medium">{t('settings.discord.bot_token')}</span>
			{#if status?.hasToken}
				<div class="mt-2 flex items-center justify-between gap-4">
					<span class="text-xs text-muted-foreground">{t('settings.discord.bot_token_saved')}</span>
					<Button variant="ghost" size="sm" disabled={saving} onclick={() => setToken('')}>
						{t('settings.discord.bot_token_remove')}
					</Button>
				</div>
			{/if}
			<div class="mt-2 flex gap-2">
				<Input
					class="h-8 font-mono text-xs"
					type="password"
					autocomplete="off"
					bind:value={token}
					placeholder={t('settings.discord.bot_token_placeholder')}
					aria-label={t('settings.discord.bot_token')}
					spellcheck={false}
					onkeydown={(e) => e.key === 'Enter' && token.trim() && !saving && setToken(token.trim())}
				/>
				<Button size="sm" disabled={!token.trim() || saving} onclick={() => setToken(token.trim())}>
					{#if saving}
						<HugeiconsIcon icon={Loading03Icon} size={15} class="animate-spin" />
					{:else}
						{t('settings.discord.bot_token_save')}
					{/if}
				</Button>
			</div>
			{#if tokenError}
				<p class="mt-1.5 text-xs text-destructive">{tokenError}</p>
			{/if}
		</div>

		<details class="px-4 py-3.5">
			<summary class="cursor-pointer text-sm font-medium marker:text-muted-foreground">
				{t('settings.discord.bot_setup')}
			</summary>
			<ol class="mt-2 list-decimal space-y-1 pl-5 text-xs leading-relaxed text-muted-foreground">
				<li>{t('settings.discord.bot_setup_1')}</li>
				<li>{t('settings.discord.bot_setup_2')}</li>
				<li>{t('settings.discord.bot_setup_3')}</li>
			</ol>
			<p class="mt-2 text-xs leading-relaxed text-muted-foreground">
				{t('settings.discord.bot_setup_note')}
			</p>
		</details>
	</div>
</section>
