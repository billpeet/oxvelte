<script lang="ts">
let {value = 1, // a comment between props
 onSave = undefined}: {value?: number; onSave?: () => void;} = $props();
onSave?.();
</script>{value}