<script lang="ts">
  // A playlist is a pile of things, so it's drawn as one: the cover with two sheet edges showing
  // above it, fanning further out under the pointer. It costs two divs and no extra requests, and
  // it's the one glance that separates "a playlist" from "an album" in a mixed shelf, which square
  // artwork alone never does.
  //
  // The sheets are full-size siblings behind an opaque cover, scaled narrower and lifted, so only
  // their top strips are ever visible. Transform-only, so the fan composites.
  import { HugeiconsIcon } from "@hugeicons/svelte";
  import {
    MusicNote01Icon,
    ListRestartIcon,
    PlayIcon,
  } from "@hugeicons/core-free-icons";
  import { ON_REPEAT_ID } from "$lib/api";
  import type { BrowseItem } from "$lib/api";
  import { thumb } from "$lib/thumb";
  import { setDragItem } from "$lib/dnd";
  import { openItem, playItem } from "$lib/browse";
  import { t } from "$lib/i18n.svelte";
  import ItemMenu from "./ItemMenu.svelte";

  let { item }: { item: BrowseItem } = $props();

  const onRepeat = $derived(item.id === ON_REPEAT_ID);

  let attempt = $state(0);
  $effect(() => {
    item.thumbnail;
    attempt = 0;
  });
  const sized = $derived(thumb(item.thumbnail, 400));
  const src = $derived(attempt === 0 ? sized : item.thumbnail);
  const imgFailed = () =>
    (attempt = attempt === 0 && sized !== item.thumbnail ? 1 : 2);
  const hasArt = $derived(!!item.thumbnail && attempt < 2 && !onRepeat);

  async function play() {
    await playItem(item);
  }
</script>

<!-- pt-3 is headroom for the lifted sheets: the shelf scrolls horizontally, which makes it clip
     vertically too, so anything reaching above the cover has to be inside the card's own box. -->
<div class="group relative w-full pt-4" data-ctx>
  <div
    class="cursor-pointer"
    role="button"
    tabindex="0"
    draggable="true"
    ondragstart={(e) => setDragItem(e, item)}
    onclick={() => openItem(item)}
    onkeydown={(e) => {
      if (e.target !== e.currentTarget) return;
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        openItem(item);
      }
    }}
    title={item.subtitle ? `${item.title} — ${item.subtitle}` : item.title}
  >
    <div class="relative aspect-square w-full">
      <div
        class="absolute inset-0 origin-bottom -translate-y-[7px] scale-x-[0.84] rounded-xl bg-muted-foreground/15 group-hover:-translate-y-[13px]"
      ></div>
      <div
        class="absolute inset-0 origin-bottom -translate-y-[3px] scale-x-[0.92] rounded-xl bg-muted-foreground/25 group-hover:-translate-y-[7px]"
      ></div>
      <!-- No resting shadow: see MediaCard. The sheet edges above are what gives the card
			     its depth, and they cost a transform instead of a gaussian blur per card. -->
      <div class="relative h-full w-full overflow-hidden rounded-xl bg-muted">
        {#if hasArt}
          <img
            {src}
            alt=""
            class="h-full w-full object-cover"
            loading="lazy"
            decoding="async"
            draggable="false"
            onerror={imgFailed}
          />
        {:else}
          <div
            class="flex h-full w-full items-center justify-center {onRepeat
              ? 'bg-primary/10 text-primary'
              : 'text-muted-foreground/50'}"
          >
            <!-- altIcon/showAlt, not a ternary: `icon` is read once at mount. -->
            <HugeiconsIcon
              icon={MusicNote01Icon}
              altIcon={ListRestartIcon}
              showAlt={onRepeat}
              class={onRepeat ? "h-10 w-10" : "h-7 w-7"}
            />
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
    </div>
    <div class="mt-2.5 min-w-0">
      <div class="truncate text-sm font-medium">{item.title}</div>
      {#if item.subtitle}
        <div class="truncate text-xs text-muted-foreground">
          {item.subtitle}
        </div>
      {/if}
    </div>
  </div>
</div>
