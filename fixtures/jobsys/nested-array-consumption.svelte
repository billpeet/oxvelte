<script lang="ts">
export type Item = {
 label: string;
 hidden?: boolean;
};
let {
 items,
 enabled = true
}: {items: Item[]; enabled?: boolean;} = $props();
const visible = $derived(items.filter(i => !i.hidden));
</script>{#each visible as item}<span>{item.label}</span>{/each}