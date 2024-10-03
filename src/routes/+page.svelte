<script lang='ts'>
	import CameraDisplay from "$lib/CameraDisplay.svelte";
	import ControlPanel from "$lib/ControlPanel.svelte";
	import { onDestroy, onMount } from "svelte";
	import { listen } from "@tauri-apps/api/event";
	import {top, front, left} from "$lib/win_store.js";
	import { invoke } from "@tauri-apps/api/tauri";

	let sources = ['','',''];
	let unlisten: (() => void) | undefined;
	const unsubscribe_top = top.subscribe(value => {
		invoke('update_camera', {winIndex: 0, camIndex: value});
	});
	const unsubscribe_front = front.subscribe(value => {
		invoke('update_camera', {winIndex: 1, camIndex: value});
	});
	const unsubscribe_left = left.subscribe(value => {
		invoke('update_camera', {winIndex: 2, camIndex: value});
	});

	onMount(async () => {
		invoke('start_streaming');

		try {
			unlisten = await listen('image-sources', (event) => {
				sources = event.payload as string[];
				console.log(sources);
			});
		} catch (e) {
			sources = ['','',''];
			unlisten = undefined;
		}
	});

	onDestroy( () => {
		if (unlisten) {
			unlisten();
		}

		unsubscribe_front();
		unsubscribe_left();
		unsubscribe_top();
	});
</script>

<div>
	<ControlPanel/>
	<CameraDisplay imageSrc={sources[0]} cameraId={$top} windowName='Top View'/>
	<CameraDisplay imageSrc={sources[1]} cameraId={$front} windowName='Front View'/>
	<CameraDisplay imageSrc={sources[2]} cameraId={$left} windowName='Left View'/>
</div>

