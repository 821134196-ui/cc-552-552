<script>
  import { STATUS_LABELS, DOOR_STATUS_LABELS } from '../api.js'
  export let status = ''
  $: label = STATUS_LABELS[status] || DOOR_STATUS_LABELS[status] || status
</script>

<span class="badge badge-{status}">{label}</span>
