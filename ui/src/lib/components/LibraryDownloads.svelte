<script lang="ts">
	import { onMount } from 'svelte';
	import TrackRow from './TrackRow.svelte';
	import TrackFilter, { filterTracks } from './TrackFilter.svelte';
	import { Button } from '$lib/components/ui/button';
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import { PlayIcon, ShuffleIcon } from '@hugeicons/core-free-icons';
	import TrackSelectionBar from './TrackSelectionBar.svelte';
	import TrackSelectButton from './TrackSelectButton.svelte';
	import { trackSelection } from '$lib/selection.svelte';
	import * as api from '$lib/api';
	import type { OfflineTrack, SongItem } from '$lib/api';
	import { openAddToPlaylist, openPlayer, playback } from '$lib/player.svelte';
	import { t } from '$lib/i18n.svelte';

	let tracks = $state<OfflineTrack[]>([]);
	let loaded = $state(false);
	let isOnline = $state(true);

	function toSong(t: OfflineTrack): SongItem {
		return {
			kind: 'song',
			video_id: t.video_id,
			title: t.title,
			artists: t.artists,
			album: t.album ?? undefined,
			duration: t.duration?.trim() || undefined,
			thumbnail: t.thumbnail ?? undefined,
			is_upload: false
		} as SongItem;
	}

	const songs = $derived(tracks.map(toSong));
	
	let query = $state('');
	const shownSongs = $derived(filterTracks(songs, query));

	const selection = trackSelection(
		() => songs,
		() => shownSongs,
		() => 'downloads',
		() => true,
		() => shownSongs.length,
		async () => true
	);

	async function removeSelected() {
		const targets = selection.songs;
		selection.exit();
		for (const target of targets) {
			await api.deleteOfflineTrack(target.video_id);
		}
		tracks = await api.getOfflineTracks();
	}

	onMount(() => {
		let unlisten: (() => void) | undefined;
		api.onDownloadProgress((p) => {
			if (p.status === 'completed' || p.status === 'deleted') {
				api.getOfflineTracks().then((t) => (tracks = t));
			}
		}).then((u) => (unlisten = u));

		api.getOfflineTracks()
			.then((t) => {
				tracks = t;
				loaded = true;
			})
			.catch(() => {
				tracks = [];
				loaded = true;
			});

		return () => {
			if (unlisten) unlisten();
		};
	});

	function play(startIndex: number) {
		openPlayer();
		return api.playPlaylist(songs, startIndex, undefined, 'Downloads');
	}

	function playAll(shuffle = false) {
		if (!shownSongs.length) return;
		openPlayer();
		api.playPlaylist(shownSongs, null, undefined, 'Downloads', shuffle);
	}
</script>

<svelte:window bind:online={isOnline} />

{#if loaded}
	{#if songs.length === 0}
		<p class="mt-8 text-center text-sm text-muted-foreground">No downloads yet.</p>
	{:else}
		<div class="mb-4 mt-2 flex flex-wrap items-center justify-between gap-3">
			<div class="flex gap-2">
				<Button
					size="sm"
					class="gap-2 rounded-full"
					disabled={!shownSongs.length}
					onclick={() => playAll(false)}
				>
					<HugeiconsIcon icon={PlayIcon} class="h-4 w-4" /> {t('common.play_all')}
				</Button>
				<Button
					size="sm"
					variant="outline"
					class="gap-2 rounded-full"
					disabled={!shownSongs.length}
					onclick={() => playAll(true)}
				>
					<HugeiconsIcon icon={ShuffleIcon} class="h-4 w-4" /> {t('common.shuffle')}
				</Button>
				<TrackSelectButton
					{selection}
					class="-ml-1 flex h-9 w-9 cursor-pointer items-center justify-center rounded-full transition hover:bg-muted hover:text-foreground"
				/>
			</div>
			<div class="relative w-full max-w-md">
				<TrackFilter
					bind:value={query}
					placeholder="Search downloads"
				/>
			</div>
		</div>
		<TrackSelectionBar
			{selection}
			from="Downloads"
			onRemove={removeSelected}
		/>
		<div class="flex flex-col">
			{#each shownSongs as song, i (song.video_id)}
				<TrackRow
					{song}
					{selection}
					selectionKey={selection.visibleKeys[i]}
					index={i}
					active={playback.now?.videoId === song.video_id}
					onplay={() => play(songs.findIndex(s => s.video_id === song.video_id))}
					onAdd={() => openAddToPlaylist(song)}
					hideRating={!isOnline}
					isOfflineList={!isOnline}
				/>
			{/each}
		</div>
	{/if}
{/if}
