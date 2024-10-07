<script lang="ts">
    import {onMount} from 'svelte';
    import {listen} from '@tauri-apps/api/event';

    export let cameraId: number;  
    export let windowName: string;
    export let winId: number;

    let error_message: string = '';
    let img_url: string;

    onMount(() => {
        let unlisten: () => void; 
        const setup_listener = async () => {
             unlisten = await listen(`image-payload-${winId}`, (event) => {
                const {image, error} = event.payload as {image: Uint8Array, error: string};
                
                if (error) {
                    error_message = error;
                } else {
                    if (!image || image.length === 0) {
                        error_message = 'No image data received';
                        return;
                    }
                    error_message = '';
                    updateUrl(URL.createObjectURL(new Blob([new Uint8Array(image).buffer])));
                }
            });
        }
        
        setup_listener();

        return () => {
            unlisten();
        }
    })

    function updateUrl(url: string) {
        if (img_url) {
            URL.revokeObjectURL(img_url);
        }
        img_url = url;
    }

</script>

<style>
img {
    max-width: 50%;
    height: auto;
}
</style>

<div>
    <h2>{windowName}</h2>
</div>
<!--TODO add display logic-->
<div>
    {#if error_message}
        <p>{error_message}</p>
    {:else}
        <img    id={`image-source-${winId}`} 
                alt={`Waiting on image from ${cameraId}`} 
                src={img_url} 
        />
    {/if}
</div>