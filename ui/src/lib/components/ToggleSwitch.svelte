<script lang="ts">
  import SquishSwitch from "./SquishSwitch.svelte";

  let {
    checked = false,
    disabled = false,
    ariaLabel = "",
    size = "md",
    onchange,
  }: {
    checked: boolean;
    disabled?: boolean;
    ariaLabel?: string;
    size?: "sm" | "md";
    onchange: (checked: boolean) => void;
  } = $props();
</script>

<SquishSwitch {checked} {disabled} {ariaLabel} {size} {onchange} />
