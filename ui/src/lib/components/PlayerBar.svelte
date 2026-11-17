<script lang="ts">
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import {
		PreviousIcon,
		NextIcon,
		PlayIcon,
		PauseIcon,
		ShuffleIcon,
		RepeatIcon,
		RepeatOne01Icon,
		Queue01Icon,
		Comment01Icon,
		Mic01Icon,
		VolumeHighIcon,
		VolumeMute02Icon,
		FavouriteIcon,
		Add01Icon,
		InfinityIcon,
		MusicNote01Icon,
		ArrowUp01Icon,
		ArrowDown01Icon,
		PreferenceVerticalIcon,
		GoBackward15SecIcon,
		GoForward15SecIcon
	} from '@hugeicons/core-free-icons';
	import { fade } from 'svelte/transition';
	import { goto } from '$app/navigation';
	import { Button } from '$lib/components/ui/button';
	import * as api from '$lib/api';
	import {
		np,
		playback,
		commitVolume,
		cycleRepeat,
		dragVolume,
		openAddToPlaylist,
		SKIP_SECS,
		toast,
		skipBy,
		toggleMute,
		toggleNowPlayingRating,
		ui,
		wheelVolume
	} from '$lib/player.svelte';
	import { thumb } from '$lib/thumb';
	import { appearance } from '$lib/theme.svelte';
	import ArtistLine from './ArtistLine.svelte';
	import M3SeekWave from './M3SeekWave.svelte';
	import Marquee from './Marquee.svelte';
	import SpeedPicker from './SpeedPicker.svelte';
	import TrackMenu from './TrackMenu.svelte';
	import DevicePicker from './metroqueue/DevicePicker.svelte';
	import { remote, remoteCmd, remoteError } from '$lib/remote.svelte';
	import { t } from '$lib/i18n.svelte';

	let {
		onToggleQueue,
		queueOpen,
		onToggleLyrics,
		lyricsOpen,
		onToggleComments,
		commentsOpen
	}: {
		onToggleQueue: () => void;
		queueOpen: boolean;
		onToggleLyrics: () => void;
		lyricsOpen: boolean;
		onToggleComments: () => void;
		commentsOpen: boolean;
	} = $props();

	// Pop the heart once when the user favourites (not when un-favouriting). Reset on animation end
	// so the next like can replay it.
	let justLiked = $state(false);

	function toggleLike() {
		if (playback.rating !== 'like') justLiked = true;
		toggleNowPlayingRating();
	}

	const fmt = (secs: number) => {
		if (!secs || secs < 0) return '0:00';
		const t = Math.floor(secs);
		const h = Math.floor(t / 3600);
		const m = Math.floor((t % 3600) / 60);
		const s = t % 60;
		const mm = h ? m.toString().padStart(2, '0') : `${m}`;
		return `${h ? `${h}:` : ''}${mm}:${s.toString().padStart(2, '0')}`;
	};

	// Remote mode (remote.svelte.ts): the bar shows the device being controlled and its controls
	// go there. Everything below reads these instead of `playback` where the two differ.
	const rs = $derived(remote.state);
	const send = (action: string, a = 0) =>
		remoteCmd(action, a).catch((e) => toast.error(remoteError(String(e))));
	const now = $derived.by(() => {
		if (!rs) return playback.now;
		const tr = rs.track;
		return tr
			? {
					title: tr.title,
					artists: tr.artist,
					artistRuns: tr.artists.map((a) => ({ text: a.name, id: a.id ?? undefined })),
					thumbnail: tr.thumb ?? tr.art ?? undefined
				}
			: null;
	});
	const paused = $derived(rs ? !rs.playing : playback.paused);
	const volume = $derived(rs ? Math.round(rs.volume * 100) : playback.volume);
	const speed = $derived(rs ? 1 : playback.speed);

	const shuffleOn = $derived(rs ? rs.shuffle : (playback.queue.shuffle ?? false));
	// Lit like shuffle while any effect is on: a speed set last week and forgotten is the case to
	// catch, since the song then just sounds wrong.
	const effectsOn = $derived(
		playback.speed !== 1 || playback.semitones !== 0 || playback.reverb.enabled
	);
	const repeat = $derived(
		rs ? (['off', 'one', 'all'] as const)[rs.repeat] ?? 'off' : (playback.queue.repeat ?? 'off')
	);
	// A podcast episode trades shuffle for speed and gets the 15-second skips either side of play.
	const episode = $derived(!rs && !!playback.now?.isEpisode);

	// The current track was appended by autoplay → show the subtle ∞ badge next to the title.
	// Matched against the now-playing videoId so a transient queue/now-playing mismatch (mid
	// gapless advance) can't flash the badge on the wrong song.
	const autoplayTrack = $derived.by(() => {
		const cur = playback.queue.items[playback.queue.currentIndex];
		return !rs && !!cur?.autoplay && cur.video_id === playback.now?.videoId;
	});

	// The ⋮ menu needs the full SongItem — NowPlaying carries no album_id. Take it from the queue
	// row, matched on videoId so a mid-advance mismatch can't point the menu at the wrong song.
	const currentSong = $derived.by(() => {
		const cur = playback.queue.items[playback.queue.currentIndex];
		return !rs && cur?.video_id === playback.now?.videoId ? cur : null;
	});

	// The title links to the song's album (there is no per-song page). Local files carry no
	// album_id, so their title stays plain text.
	const albumId = $derived(
		currentSong && !api.isLocalId(currentSong.video_id) ? currentSong.album_id : undefined
	);

	// Seek: while dragging, hold a local value so incoming mpv position ticks can't yank the thumb
	// back under the pointer; only invoke the (expensive) seek on release.
	let seekDrag = $state<number | null>(null);
	// Display playback seconds at the selected tempo; mpv seeks still take track seconds.
	const shownPosition = $derived((seekDrag ?? (rs ? remote.position : playback.position)) / speed);
	const shownDuration = $derived((rs ? rs.duration / 1000 : playback.duration) / speed);
	const seekPct = $derived(
		shownDuration > 0 ? Math.min(1, Math.max(0, shownPosition / shownDuration)) : 0
	);

	function onSeekInput(e: Event) {
		seekDrag = Number((e.target as HTMLInputElement).value) * speed;
	}
	function onSeekCommit(e: Event) {
		const v = Number((e.target as HTMLInputElement).value) * speed;
		seekDrag = null;
		if (rs) {
			remote.position = v;
			send('seek', v * 1000);
			return;
		}
		playback.position = v;
		api.seek(v);
	}

	const onVolume = (e: Event) => {
		const v = Number((e.target as HTMLInputElement).value);
		if (rs) rs.volume = v / 100;
		else dragVolume(v);
	};
	const onVolumeCommit = (e: Event) => {
		const v = Number((e.target as HTMLInputElement).value);
		if (rs) send('volume', v);
		else commitVolume(v);
	};
	const onMute = () =>
		rs ? send('volume', rs.volume > 0 ? 0 : remote.lastVolume) : toggleMute();

	const isControl = (t: EventTarget | null) =>
		!!(t as HTMLElement | null)?.closest?.('button, a, input, [role="button"]');

	// Dragging a slider past its end and releasing outside it retargets the click at the bar (the
	// click lands on the common ancestor of press and release), which used to toggle the view.
	// So judge by where the press started, not where the release happened.
	let pressedControl = false;

	// Anywhere on the bar that isn't a control opens (or closes) the now-playing view: the bar is
	// what's left of it once it's minimised, so it's the way back in. Deliberately no pointer
	// cursor, because this is the whole bar, not a button, and every real button keeps its own click.
	// Settings can switch it off for whoever keeps tripping it on a missed control.
	function onBarClick(e: MouseEvent) {
		if (!appearance.emptyClickTogglesPlayer || pressedControl || isControl(e.target)) return;
		np.open = !np.open;
	}
</script>

<!-- The chevron button below is the keyboard equivalent of clicking the bar, so the bar itself
     stays a plain region rather than becoming a focusable control wrapping every other control. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions, a11y_no_noninteractive_element_interactions -->
<footer
	data-player-bar
	data-playing={paused ? 'false' : 'true'}
	onpointerdown={(e) => (pressedControl = isControl(e.target))}
	onclick={onBarClick}
	class="mx-2 mb-2 flex items-center gap-2 bg-card px-2 py-2 sm:gap-4 sm:px-3"
>
	<!-- Now playing. data-ctx: right-clicking the cover or the title opens the ⋮ menu for the track
	     that's playing (not the buttons beside them — those keep their own meaning). -->
	<div class="flex min-w-0 flex-1 items-center gap-3" data-ctx>
		{#key rs ? rs.track?.id : playback.now?.videoId}
			{#if now?.thumbnail}
				<img
					data-player-art
					src={thumb(now.thumbnail, 120)}
					alt=""
					style="max-width:none"
					class="h-14 w-14 shrink-0 rounded-md object-cover"
					in:fade={{ duration: 250 }}
				/>
			{:else}
				<div
					data-player-art
					class="flex h-14 w-14 shrink-0 items-center justify-center rounded-md bg-surface-container-highest text-on-surface-variant"
				>
					<HugeiconsIcon icon={MusicNote01Icon} class="h-5 w-5" />
				</div>
			{/if}
		{/key}
		<div class="min-w-0">
			<div class="flex items-center gap-1.5">
				{#snippet title()}
					<Marquee
						text={now?.title ?? t('player.not_playing')}
						class="text-title-md"
					/>
				{/snippet}
				<!-- The button wraps the whole marquee rather than the text inside it: mid-scroll the
				     visible half of the line is the inert trailing copy, so a button around the text
				     itself is only clickable while the original copy is on screen. The underline goes
				     on the spans, not the button: text-decoration doesn't reach the trailing copy,
				     which is absolutely positioned. -->
				{#if albumId}
					<button
						class="min-w-0 cursor-pointer text-left hover:[&_span]:underline"
						onclick={() => goto(`/album/${encodeURIComponent(albumId)}`)}
					>
						{@render title()}
					</button>
				{:else}
					{@render title()}
				{/if}
				{#if autoplayTrack}
					<span
						class="shrink-0 text-muted-foreground"
						title={t('player.autoplay_notice')}
						in:fade={{ duration: 200 }}
					>
						<HugeiconsIcon icon={InfinityIcon} class="h-3.5 w-3.5" />
					</span>
				{/if}
			</div>
			<ArtistLine
				runs={now?.artistRuns}
				text={now?.artists ?? ''}
				marquee
				class="block max-w-full text-body-md text-on-surface-variant"
			/>
		</div>
		{#if playback.now && !rs}
			<div class="flex items-center">
				<!-- A local file has no YouTube identity (see api.isLocalId): nothing to like. It can
				     still go in a playlist on this machine, so + stays. Below lg both drop and the ⋮
				     menu carries them instead: on a narrow window three buttons here leave the title
				     almost no room. lg, not md: the window's minWidth is 900 (tauri.conf.json), so md
				     never fires. -->
				{#if !api.isLocalId(playback.now.videoId)}
					<Button
						variant="ghost"
						size="icon-sm"
						class="hidden lg:inline-flex"
						onclick={toggleLike}
						aria-label={t('common.like')}
					>
						<span
							class="inline-flex"
							class:animate-heart-pop={justLiked}
							onanimationend={() => (justLiked = false)}
						>
							<HugeiconsIcon
								icon={FavouriteIcon}
								class="h-4 w-4 {playback.rating === 'like' ? 'fill-current text-primary' : 'text-muted-foreground'}"
							/>
						</span>
					</Button>
				{/if}
				<Button
					variant="ghost"
					size="icon-sm"
					class="hidden lg:inline-flex"
					onclick={() => {
						// The queue's row when it is there: it carries the album and the artist links
						// that a playlist on this machine keeps, and the now-playing event does not.
						const now = playback.now!;
						openAddToPlaylist(
							currentSong ?? {
								video_id: now.videoId,
								title: now.title,
								artists: now.artists,
								artist_id: now.artistId,
								thumbnail: now.thumbnail,
								duration: now.duration
							}
						);
					}}
					aria-label={t('player.save_to_playlist')}
				>
					<HugeiconsIcon icon={Add01Icon} class="h-4 w-4 text-muted-foreground" />
				</Button>
				{#if currentSong}
					<TrackMenu
						song={currentSong}
						linksOnly
						playlistId={playback.queue.sourceId}
						queueIndex={playback.queue.currentIndex}
						onAdd={() => openAddToPlaylist(currentSong!)}
						triggerClass="state-layer inline-flex size-8 cursor-pointer items-center justify-center rounded-full text-on-surface-variant"
					/>
				{/if}
			</div>
		{/if}
	</div>

	<!-- Transport -->
	<div class="flex flex-[1.5] flex-col items-center gap-1">
		<div class="flex items-center gap-1">
			{#if episode}
				<SpeedPicker
					class="inline-flex h-8 min-w-8 items-center justify-center rounded-4xl px-0.5 text-xs hover:bg-muted dark:hover:bg-muted/50 aria-expanded:bg-muted"
				/>
			{:else}
				<Button
					variant="ghost"
					size="icon-sm"
					onclick={() => (rs ? send('shuffle') : api.toggleShuffle())}
					aria-label={t('player.shuffle')}
					aria-pressed={shuffleOn}
				>
					<HugeiconsIcon
						icon={ShuffleIcon}
						class="h-4 w-4 {shuffleOn ? 'text-primary' : 'text-muted-foreground'}"
					/>
				</Button>
			{/if}
			<Button variant="ghost" size="icon-sm" onclick={() => (rs ? send('prev') : api.prevTrack())} aria-label={t('player.previous')}>
				<HugeiconsIcon icon={PreviousIcon} class="h-5 w-5" />
			</Button>
			{#if episode}
				<Button
					variant="ghost"
					size="icon-sm"
					onclick={() => skipBy(-SKIP_SECS)}
					aria-label={t('player.skip_back', { seconds: SKIP_SECS })}
					title={t('player.skip_back', { seconds: SKIP_SECS })}
				>
					<HugeiconsIcon icon={GoBackward15SecIcon} class="h-5 w-5" />
				</Button>
			{/if}
			<Button
				variant="secondary"
				size="icon"
				data-play-button
				class="mx-1"
				onclick={() => (rs ? send(rs.playing ? 'pause' : 'resume') : api.togglePause())}
				aria-label={paused ? t('player.play') : t('player.pause')}
			>
				<!-- HugeiconsIcon only re-renders `altIcon`/`showAlt`, not `icon` (frozen at mount) —
			     so toggle via showAlt, not a ternary on `icon`. -->
			<HugeiconsIcon
				icon={PauseIcon}
				altIcon={PlayIcon}
				showAlt={paused}
				class="h-6 w-6"
			/>
			</Button>
			{#if episode}
				<Button
					variant="ghost"
					size="icon-sm"
					onclick={() => skipBy(SKIP_SECS)}
					aria-label={t('player.skip_forward', { seconds: SKIP_SECS })}
					title={t('player.skip_forward', { seconds: SKIP_SECS })}
				>
					<HugeiconsIcon icon={GoForward15SecIcon} class="h-5 w-5" />
				</Button>
			{/if}
			<Button variant="ghost" size="icon-sm" onclick={() => (rs ? send('next') : api.nextTrack())} aria-label={t('player.next')}>
				<HugeiconsIcon icon={NextIcon} class="h-5 w-5" />
			</Button>
			<Button
				variant="ghost"
				size="icon-sm"
				onclick={() => (rs ? send('repeat') : cycleRepeat())}
				aria-label={t('player.repeat_state', {
					state: repeat === 'off' ? t('player.repeat_off') : repeat === 'one' ? t('player.repeat_one') : t('player.repeat_all')
				})}
				aria-pressed={repeat !== 'off'}
			>
				<!-- icon swap via altIcon/showAlt — `icon` is frozen at mount (see play/pause above) -->
				<HugeiconsIcon
					icon={RepeatIcon}
					altIcon={RepeatOne01Icon}
					showAlt={repeat === 'one'}
					class="h-4 w-4 {repeat !== 'off' ? 'text-primary' : 'text-muted-foreground'}"
				/>
			</Button>
		</div>
		<div class="flex w-full max-w-md items-center gap-2 text-label-md text-on-surface-variant">
			<!-- Fixed slots, or the seek bar twitches as the position ticks: DM Sans has no tabular
			     figures, so `tabular-nums` does nothing there (#463). -->
			<span class="min-w-[5ch] text-right tabular-nums">{fmt(shownPosition)}</span>
			<div
				class="m3-seek relative flex flex-1 items-center"
				style="--pct:{seekPct * 100}%"
			>
				<M3SeekWave
					pct={seekPct}
					paused={paused}
					duration={shownDuration}
					dragging={seekDrag !== null}
				/>
				<input
					type="range"
					class="range w-full flex-1"
					style="--pct:{seekPct * 100}%"
					min="0"
					max={shownDuration || 0}
					value={shownPosition}
					oninput={onSeekInput}
					onchange={onSeekCommit}
					aria-label={t('player.seek')}
				/>
			</div>
			<span class="min-w-[5ch] tabular-nums">{fmt(shownDuration)}</span>
		</div>
	</div>

	<!-- Volume + queue -->
	<div class="flex flex-1 items-center justify-end gap-2">
		<!-- Volume is the first control to drop on a narrow window (OS volume still works). -->
		<div class="hidden items-center gap-1 md:flex">
			<Button
				variant="ghost"
				size="icon-sm"
				class="text-muted-foreground"
				onclick={onMute}
				aria-label={volume === 0 ? t('player.unmute') : t('player.mute')}
			>
				<!-- icon swap via altIcon/showAlt — `icon` is frozen at mount (see play/pause above) -->
				<HugeiconsIcon
					icon={VolumeHighIcon}
					altIcon={VolumeMute02Icon}
					showAlt={volume === 0}
					class="h-4 w-4"
				/>
			</Button>
			<input
				type="range"
				class="range w-24"
				style="--pct:{volume}%"
				min="0"
				max="100"
				value={volume}
				oninput={onVolume}
				onchange={onVolumeCommit}
				onwheel={rs ? undefined : wheelVolume}
				aria-label={t('player.volume')}
			/>
		</div>
		<!-- One cluster, so they sit tighter to each other than to the volume slider. -->
		<div class="flex items-center gap-0.5">
			<DevicePicker />
			<Button
				variant={ui.tempoPitchOpen ? 'secondary' : 'ghost'}
				size="icon-sm"
				onclick={() => (ui.tempoPitchOpen = !ui.tempoPitchOpen)}
				aria-label={t('dialogs.tempo_pitch.title')}
				aria-expanded={ui.tempoPitchOpen}
			>
				<HugeiconsIcon
					icon={PreferenceVerticalIcon}
					class="h-5 w-5 {effectsOn ? 'text-primary' : ''}"
				/>
			</Button>
			<Button
				variant={lyricsOpen ? 'secondary' : 'ghost'}
				size="icon-sm"
				onclick={onToggleLyrics}
				aria-label={t('player.lyrics')}
			>
				<HugeiconsIcon icon={Mic01Icon} class="h-5 w-5" />
			</Button>
			<!-- A local file has no YouTube comment thread to open. -->
			{#if playback.now && !api.isLocalId(playback.now.videoId)}
				<Button
					variant={commentsOpen ? 'secondary' : 'ghost'}
					size="icon-sm"
					onclick={onToggleComments}
					aria-label={t('player.comments')}
				>
					<HugeiconsIcon icon={Comment01Icon} class="h-5 w-5" />
				</Button>
			{/if}
			<Button
				variant={queueOpen ? 'secondary' : 'ghost'}
				size="icon-sm"
				onclick={onToggleQueue}
				aria-label={t('player.queue')}
			>
				<HugeiconsIcon icon={Queue01Icon} class="h-5 w-5" />
			</Button>
			<!-- The keyboard (and discoverable) way in and out of the now-playing view; clicking the
			     bar's empty space does the same thing. -->
			<Button
				variant="ghost"
				size="icon-sm"
				onclick={() => (np.open = !np.open)}
				aria-label={np.open ? t('player.minimize_player') : t('player.open_player')}
				aria-expanded={np.open}
			>
				<!-- icon swap via altIcon/showAlt — `icon` is frozen at mount (see play/pause above) -->
				<HugeiconsIcon
					icon={ArrowUp01Icon}
					altIcon={ArrowDown01Icon}
					showAlt={np.open}
					class="h-5 w-5"
				/>
			</Button>
		</div>
	</div>
</footer>
