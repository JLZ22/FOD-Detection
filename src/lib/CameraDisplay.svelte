<script lang="ts">
    import { onMount } from 'svelte';
    import { listen } from '@tauri-apps/api/event';
    import { invoke } from '@tauri-apps/api/tauri';
  
    export let cameraId: number;  // Component receives the camera ID as a prop
    let imageSrc = '';  // Store the current frame's Base64 image data
  
    onMount(() => {
      // Listen for the frame event corresponding to this camera
      listen(`camera_${cameraId}_frame`, event => {
        imageSrc = `${event.payload}`; // Ensure the event payload includes 'data:image/png;base64,' before the encoaded image data
      });
  
      // Start the camera stream for this camera
      invoke('start_streaming', { camera_id: cameraId });
    });
  </script>
  
  <style>
    img {
      max-width: 100%;
      height: auto;
    }
  </style>
  
  {#if imageSrc}
    <img src={imageSrc} alt={`Camera ${cameraId} Stream`} />
  {:else}
    <p>Waiting for camera {cameraId} stream...</p>
  {/if}
  