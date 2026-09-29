<script lang="ts">
  import { cn, type WithElementRef } from "$lib/utils.js";
  import type { HTMLAttributes } from "svelte/elements";

  let {
    class: className,
    ref = $bindable(null),
    pulse = false,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
    pulse?: boolean;
  } = $props();
</script>

<div
  bind:this={ref}
  data-slot="skeleton"
  class={cn(
    "bg-muted rounded-md",
    pulse ? "animate-pulse" : "animate-none opacity-70",
    className,
  )}
  {...restProps}
></div>
