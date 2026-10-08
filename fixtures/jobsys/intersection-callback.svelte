<script lang="ts">
type Props = {onSave?: () => void} & {missing: string};
let {onSave, enabled}: Props = $props();
onSave?.();
</script>