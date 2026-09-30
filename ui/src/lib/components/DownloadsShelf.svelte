<script lang="ts">
	// Downloaded songs shelf for the home page — always local, works offline.
	// Shows tracks saved for offline playback (via ⋯ → Download), in the order
	// they were downloaded, newest first.  Playing one queues the whole shelf so
	// the user can listen without a connection the same way Spotify/Apple Music do.
	import { onMount } from 'svelte';
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import { Download01Icon, NoInternetIcon } from '@hugeicons/core-free-icons';
	import SectionHeading from './SectionHeading.svelte';
	import TrackRow from './TrackRow.svelte';
	import * as api from '$lib/api';
	import type { OfflineTrack, SongItem } from '$lib/api';
	import { openAddToPlaylist, openPlayer, playback } from '$lib/player.svelte';
	import { goto } from '$app/navigation';

	let tracks = $state<OfflineTrack[]>([]);
	let loaded = $state(false);
	let isOnline = $state(true);

	// Convert an OfflineTrack to the SongItem shape the player expects.
	function toSong(t: OfflineTrack): SongItem {
		return {
			kind: 'song',
			video_id: t.video_id,
			title: t.title,
			artists: t.artists,
			album: t.album ?? undefined,
			duration: t.duration ?? undefined,
			thumbnail: t.thumbnail ?? undefined,
			is_upload: false
		} as SongItem;
	}

	const songs = $derived(tracks.map(toSong));

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
</script>

<svelte:window bind:online={isOnline} />

{#if loaded && songs.length}
	<section>
		<SectionHeading
			title="Downloads"
			icon={Download01Icon}
			onMore={() => goto('/library?tab=downloads')}
			moreLabel="See all"
		/>
		<!-- CSS columns: fills top-to-bottom, balances the last column, matches
		     the Forgotten Favourites layout for consistent visual language. -->
		<div class="columns-1 gap-x-6 md:columns-2 xl:columns-3">
			{#each songs as song, i (song.video_id)}
				<div class="break-inside-avoid">
					<TrackRow
						{song}
						index={i}
						compact
						active={playback.now?.videoId === song.video_id}
						onplay={() => play(i)}
						onAdd={() => openAddToPlaylist(song)}
						hideRating={!isOnline}
						isOfflineList={!isOnline}
					/>
				</div>
			{/each}
		</div>
	</section>
{/if}
