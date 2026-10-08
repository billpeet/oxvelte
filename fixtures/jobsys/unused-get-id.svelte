<script lang="ts">
interface Props {
 getDisplay?: (item: string) => string | null | undefined;
 getId?: (item: string) => string | null | undefined;
}
let {getDisplay}: Props = $props();
</script>{getDisplay?.("a")}