<script lang="ts">
interface Props {
 getDisplay?: (item: string) => string | null | undefined;
 placeholder?: string;
 getId?: (item: string) => string | null | undefined;
}
let {getDisplay, placeholder = "Select..."}: Props = $props();
</script>{getDisplay?.("a")}