<script lang="ts">
    export let cameraId: number;  // Component receives the camera ID as a prop
    export let imageSrc: string;  // Store the current frame's Base64 image data
</script>

<style>
img {
    max-width: 100%;
    height: auto;
}
</style>

{#if imageSrc}
<img src={imageSrc} alt={`Camera ${cameraId + 1} Stream`} />
{:else}
<p>Waiting for camera {cameraId} stream...</p>
{/if}
