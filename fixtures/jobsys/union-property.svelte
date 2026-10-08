<script lang="ts">
let {onSave}: {
 onSave?:
  | ((value: number) => void)
  | undefined;
} = $props();
</script>
<button onclick={() => onSave?.(1)}>Save</button>