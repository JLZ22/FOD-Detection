<script lang='ts'>
	import CameraDisplay from "$lib/CameraDisplay.svelte";
	import ControlPanel from "$lib/ControlPanel.svelte";
	import { invoke } from '@tauri-apps/api/tauri';
	import { onMount } from "svelte";

	let sources: string[] = ['','',''];

	onMount(async () => {
		sources = await invoke('start_streaming', {})
	});
</script>

<div>
	<ControlPanel/>
	<CameraDisplay imageSrc={sources[0]} cameraId={0} windowName='Top View'/>
	<CameraDisplay imageSrc={sources[1]} cameraId={1} windowName='Front View'/>
	<CameraDisplay imageSrc={sources[2]} cameraId={2} windowName='Left View'/>
</div>

