<script lang="ts">
let {
 details,
 enabled = true
}: {details: {value: string}; enabled?: boolean} = $props();
</script>{details?.value}