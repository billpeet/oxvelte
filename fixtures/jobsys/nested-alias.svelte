<script lang="ts">
let {
 details,
 enabled = true
}: {details: {used: string; hidden: string}; enabled?: boolean} = $props();
const alias = details;
console.log(alias.used);
</script>