<script lang='ts'>
	import CameraDisplay from "$lib/CameraDisplay.svelte";
	import ControlPanel from "$lib/ControlPanel.svelte";
	import { onDestroy, onMount } from "svelte";
	import { listen } from "@tauri-apps/api/event";
	import {win1} from "$lib/win1.js";
	import {win2} from "$lib/win2.js";
	import {win3} from "$lib/win3.js";
	import { invoke } from "@tauri-apps/api/tauri";

	let sources = ['','',''];
	let unlisten: (() => void) | undefined;
	const unsubscribe_win1 = win1.subscribe(value => {
		invoke('update_win_camera', {win: 1, index: value})
	});
	const unsubscribe_win2 = win2.subscribe(value => {
		invoke('update_win_camera', {win: 2, index: value})
	});
	const unsubscribe_win3 = win3.subscribe(value => {
		invoke('update_win_camera', {win: 3, index: value})
	});

	onMount(async () => {

		try {
			unlisten = await listen('image-sources', (event) => {
				sources = event.payload as string[];
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

		unsubscribe_win1();
		unsubscribe_win2();
		unsubscribe_win3();
	});
</script>

<div>
	<ControlPanel/>
	<CameraDisplay imageSrc={sources[0]} cameraId={$win1} windowName='Top View'/>
	<CameraDisplay imageSrc={sources[1]} cameraId={$win2} windowName='Front View'/>
	<CameraDisplay imageSrc={sources[2]} cameraId={$win3} windowName='Left View'/>
</div>

