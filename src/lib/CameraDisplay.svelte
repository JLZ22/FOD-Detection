<script lang="ts">
    export let cameraId: number;  // Component receives the camera ID as a prop
    export let imageSrc: string;  // Store the current frame's Base64 image data
    export let windowName: string;
</script>

<style>
img {
    max-width: 100%;
    height: auto;
}
</style>

<div>
<h2>{windowName}</h2>
</div>

{#if imageSrc.length > 0}
{#if imageSrc.includes('Error')}
    <p>{imageSrc} Camera source: {cameraId}</p>
{:else}
    <img src={imageSrc} alt={`Waiting on camera index: ${cameraId} ...`} />
{/if}
{:else}
<p>Waiting for camera {cameraId} stream...</p>
{/if}
