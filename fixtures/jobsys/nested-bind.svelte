<script lang="ts">
let {
 details = $bindable(),
 enabled = true
}: {details: {value: string}; enabled?: boolean} = $props();
</script><Child bind:details />