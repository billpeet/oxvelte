<script lang="ts">
let {value = 1, // if true, calls a helper; if false, calls onSave
 onSave = undefined}: {value?: number; onSave?: () => void;} = $props();
onSave?.();
</script>{value}