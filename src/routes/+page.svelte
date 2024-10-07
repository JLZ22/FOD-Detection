<script lang='ts'>
	import CameraDisplay from "$lib/CameraDisplay.svelte";
	import ControlPanel from "$lib/ControlPanel.svelte";
	import { onMount } from "svelte";
	import {top, front, left} from "$lib/win_store.js";
	import { invoke } from "@tauri-apps/api/tauri";
	
	onMount(() => {
		invoke('start_streaming');
		
		const unsubscribe_top = top.subscribe(value => {
			invoke('update_camera', {winIndex: 0, camIndex: value});
		});
		const unsubscribe_front = front.subscribe(value => {
			invoke('update_camera', {winIndex: 1, camIndex: value});
		});
		const unsubscribe_left = left.subscribe(value => {
			invoke('update_camera', {winIndex: 2, camIndex: value});
		});


		return () => {
			unsubscribe_top();
			unsubscribe_front();
			unsubscribe_left();
		};
	});
</script>

<div>
	<ControlPanel/>
	<CameraDisplay cameraId={$top} windowName='Top View' winId={0}/>
	<CameraDisplay cameraId={$front} windowName='Front View' winId={1}/>
	<CameraDisplay cameraId={$left} windowName='Left View' winId={2}/>
</div>

