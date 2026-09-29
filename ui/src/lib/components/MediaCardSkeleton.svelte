<script lang="ts">
  import { Skeleton } from "$lib/components/ui/skeleton";

  // Mirrors MediaCard's layout so the grid doesn't reflow when real data arrives.
  let { round = false }: { round?: boolean } = $props();
</script>

<div class="flex w-full flex-col gap-2 p-2">
  <Skeleton
    pulse={false}
    class="aspect-square w-full {round ? 'rounded-full' : 'rounded-lg'}"
  />
  <div class="{round ? 'flex flex-col items-center' : ''} space-y-1.5">
    <Skeleton pulse={false} class="h-3.5 w-3/4 rounded" />
    <Skeleton pulse={false} class="h-3 w-1/2 rounded" />
  </div>
</div>
