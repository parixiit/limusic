<script lang="ts">
  // A "From the community" playlist card: cover, title, a peek at the first three tracks, and
  // play / add-to-playlist. Wider than a MediaCard, so the shelf stretches these instead of
  // packing more of them per row (see Shelf's `community` prop).
  import { goto } from "$app/navigation";
  import { HugeiconsIcon } from "@hugeicons/svelte";
  import { PlayIcon, MusicNote01Icon } from "@hugeicons/core-free-icons";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import * as api from "$lib/api";
  import type { BrowseItem, PlaylistPage } from "$lib/api";
  import { thumb } from "$lib/thumb";
  import { getCached, putCached } from "$lib/pagecache";
  import { playFrom, touchPick } from "$lib/player.svelte";
  import { t } from "$lib/i18n.svelte";
  import ItemMenu from "./ItemMenu.svelte";

  let { item }: { item: BrowseItem } = $props();

  // A community playlist's "cover" is usually just the creator's channel avatar — often the
  // generated first-letter tile. Those are served from yt3.*; a real playlist cover is an
  // i.ytimg video thumb or lh3 playlist art. When it's an avatar, build the cover from the
  // tracks' own artwork instead (2×2, like every other multi-track cover in the app).
  const avatar = $derived(/\/\/yt3\./.test(item.thumbnail ?? ""));

  let pl = $state<PlaylistPage | null>(null);
  let root = $state<HTMLElement | null>(null);

  // Same key the playlist page uses — a card that loaded makes opening it instant, and vice versa.
  const key = $derived(`playlist:${item.id}`);
  let loadedOnce = $state(false);

  async function load(): Promise<PlaylistPage> {
    if (pl) return pl;
    const hit = getCached<PlaylistPage>(key);
    if (hit) return (pl = hit);
    const fresh = await api.getPlaylist(item.id);
    putCached(key, fresh);
    return (pl = fresh);
  }

  // Every card is one browse call, and a shelf holds 20 — only spend it once the card is on screen
  // long enough to settle. A fast swipe used to fire dozens of fetches while the shelf was still
  // moving, which is what made the community row feel sticky. Defer the request a little so the
  // scroll stays smooth while the user is still moving.
  let loading = $state(false);
  let pendingTimer: number | undefined = undefined;
  $effect(() => {
    if (!root || loadedOnce || loading) return;
    const io = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) {
        if (pendingTimer !== undefined) {
          window.clearTimeout(pendingTimer);
          pendingTimer = undefined;
        }
        return;
      }
      if (pendingTimer !== undefined) return;
      pendingTimer = window.setTimeout(() => {
        pendingTimer = undefined;
        if (loadedOnce || loading || !root) return;
        loading = true;
        load()
          .catch(() => {})
          .finally(() => {
            loading = false;
            loadedOnce = true;
          });
      }, 120);
    });
    io.observe(root);
    return () => {
      if (pendingTimer !== undefined) {
        window.clearTimeout(pendingTimer);
        pendingTimer = undefined;
      }
      io.disconnect();
    };
  });

  const tracks = $derived(pl?.items.slice(0, 3) ?? []);
  // Distinct covers only: playlists of art tracks repeat one album's artwork, and a mosaic of four
  // identical tiles looks broken. Under four, a single track cover still beats a letter avatar.
  const covers = $derived([
    ...new Set(
      (pl?.items ?? []).map((s) => s.thumbnail).filter((t): t is string => !!t),
    ),
  ]);
  const mosaic = $derived(
    avatar && covers.length >= 4 ? covers.slice(0, 4) : [],
  );
  const cover = $derived(
    avatar ? (covers[0] ?? item.thumbnail) : item.thumbnail,
  );
  // The playlist header's own subtitle is "31K views • 46 tracks • 3 hours, 11 minutes"; the card's
  // subtitle already says "Creator • 31K views", so drop the duplicated views run.
  const stats = $derived(
    pl?.subtitle?.replace(/^[^•]*views\s*•\s*/i, "") ?? "",
  );

  async function play() {
    await playFrom(
      item,
      pl?.items ?? [],
      0,
      item.id,
      undefined,
      pl?.continuation,
    );
  }

  function open() {
    touchPick(item.id);
    goto(`/playlist/${encodeURIComponent(item.id)}`);
  }
</script>

<div
  bind:this={root}
  data-ctx
  class="group relative flex h-full min-w-0 flex-col gap-2 rounded-2xl bg-foreground/5 p-2.5 transition-colors hover:bg-foreground/10"
>
  <!-- A div, not a button: `ctxHost` treats a nested <button> as its own thing and would leave the
	     card's whole cover-and-title block without a right-click menu. -->
  <div
    class="block w-full min-w-0 cursor-pointer"
    role="button"
    tabindex="0"
    onclick={open}
    onkeydown={(e) => {
      if (e.target !== e.currentTarget) return;
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        open();
      }
    }}
    title={item.title}
  >
    <!-- No shadow at rest, and none faded in on hover: transitioning shadow-sm to shadow-lg makes
		     WebKit compute a different gaussian blur every frame, for every card in the repainted
		     tile. The card's background already answers the hover. -->
    <div
      class="relative mx-auto aspect-square w-full max-w-44 overflow-hidden rounded-xl bg-muted"
    >
      {#if mosaic.length === 4}
        <div class="grid h-full w-full grid-cols-2 grid-rows-2">
          {#each mosaic as m (m)}
            <img
              src={thumb(m, 200)}
              alt=""
              class="h-full w-full object-cover"
              loading="lazy"
              decoding="async"
              fetchpriority="low"
            />
          {/each}
        </div>
      {:else if cover}
        <img
          src={thumb(cover, 400)}
          alt=""
          class="h-full w-full object-cover"
          loading="lazy"
          decoding="async"
          fetchpriority="low"
        />
      {:else}
        <div
          class="flex h-full w-full items-center justify-center text-muted-foreground/50"
        >
          <HugeiconsIcon icon={MusicNote01Icon} class="h-6 w-6" />
        </div>
      {/if}
      <button
        class="absolute bottom-2 right-2 flex h-8 w-8 cursor-pointer items-center justify-center rounded-full bg-primary text-primary-foreground opacity-0 shadow-md transition-opacity group-hover:opacity-100 hover:brightness-110 focus-visible:opacity-100"
        aria-label={t("a11y.play_item", { title: item.title })}
        onclick={(e) => {
          e.stopPropagation();
          play();
        }}
      >
        <HugeiconsIcon icon={PlayIcon} class="h-3.5 w-3.5" />
      </button>
      <ItemMenu
        {item}
        triggerClass="absolute right-2 top-2 flex h-8 w-8 cursor-pointer items-center justify-center rounded-full bg-background/80 text-muted-foreground opacity-0 transition hover:bg-muted hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100"
      />
    </div>
    <div class="mt-2 truncate text-center text-sm font-medium">
      {item.title}
    </div>
    {#if item.subtitle}
      <div class="truncate text-center text-xs text-muted-foreground">
        {item.subtitle}
      </div>
    {/if}
    {#if stats}
      <div
        class="truncate text-center text-[0.6875rem] text-muted-foreground/70"
      >
        {stats}
      </div>
    {/if}
  </div>

  <div class="flex min-w-0 flex-col gap-0.5">
    {#if tracks.length}
      {#each tracks as t, i (t.video_id + ":" + i)}
        <button
          class="flex w-full min-w-0 cursor-pointer items-center gap-2 rounded-lg p-1 text-left transition-colors hover:bg-accent/10"
          onclick={() =>
            playFrom(item, pl!.items, i, item.id, undefined, pl!.continuation)}
          title={t.artists ? `${t.title} — ${t.artists}` : t.title}
        >
          {#if t.thumbnail}
            <img
              src={thumb(t.thumbnail, 100)}
              alt=""
              class="h-8 w-8 shrink-0 rounded-md bg-muted object-cover"
              loading="lazy"
              decoding="async"
              fetchpriority="low"
            />
          {:else}
            <div class="h-8 w-8 shrink-0 rounded-md bg-muted"></div>
          {/if}
          <!-- flex-1, not a bare min-w-0: with flex-basis left at auto the wrapper asks for its
					     content's width and a long title pushes the row wide instead of ellipsing. The
					     skeleton rows below always had this; the real ones did not. -->
          <span class="min-w-0 flex-1">
            <span class="block truncate text-xs font-medium">{t.title}</span>
            <span class="block truncate text-[0.6875rem] text-muted-foreground"
              >{t.artists}</span
            >
          </span>
        </button>
      {/each}
    {:else}
      <!-- Keep the placeholder still while the community playlist is loading: the pulse animation is
			     cheap on one card, but a shelf with a dozen cards active at once multiplies the animation
			     cost and makes scrolling feel sticky. A soft static opacity is enough to tell users the
			     card is still loading without fighting the scroll. -->
      {#each Array(3) as _, i (i)}
        <div class="flex items-center gap-2 p-1">
          <Skeleton
            class="h-8 w-8 shrink-0 rounded-md animate-none opacity-70"
          />
          <div class="min-w-0 flex-1">
            <Skeleton class="mb-1 h-3 w-3/5 rounded animate-none opacity-70" />
            <Skeleton class="h-2.5 w-2/5 rounded animate-none opacity-70" />
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>
