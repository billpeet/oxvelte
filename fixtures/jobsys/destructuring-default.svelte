<script lang="ts">
type Size = "sm" | "md";
let {size = "md", animated = false, class: className = ""}: {size?: Size; animated?: boolean; class?: string;} = $props();
</script><span class={className}>{size}{animated}</span>