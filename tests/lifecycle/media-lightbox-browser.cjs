// Run against a local Vite dev server; no node credentials or user media involved.
const { chromium, expect } = require('../../neode-ui/node_modules/@playwright/test');
const base = process.env.LIGHTBOX_TEST_URL || 'http://127.0.0.1:5187';
(async () => {
 const browser = await chromium.launch({ headless: true });
 try {
  for (const [width,height] of [[320,568],[390,844],[844,390],[1440,900]]) {
   const context = await browser.newContext({viewport:{width,height},hasTouch:width<900});
   const page = await context.newPage();
   const errors=[];page.on('pageerror',e=>errors.push(e.message));
   await page.route('**/lightbox-fixture',r=>r.fulfill({contentType:'text/html',body:`<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><div id="fixture"></div><script type="module">
    import {createApp,h,ref} from '/node_modules/.vite/deps/vue.js';
    import Viewer from '/src/components/cloud/MediaLightbox.vue';
    import '/src/style.css';
    document.documentElement.style.setProperty('--safe-area-top','28px');
    document.documentElement.style.setProperty('--safe-area-bottom','24px');
    const svg='<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="800"><rect width="1200" height="800" fill="#123b3b"/><circle cx="600" cy="400" r="200" fill="#79dbc8"/></svg>';
    const url=URL.createObjectURL(new Blob([svg],{type:'image/svg+xml'}));
    window.show=ref(true);window.start=ref(0);
    const items=[{name:'A very long photo name that must truncate without shrinking any buttons.jpg',path:'/one.jpg'},{name:'second.jpg',path:'/two.jpg'},{name:'Landscape video with a long filename.webm',path:'/video.webm'}];
    createApp({render:()=>h(Viewer,{items,startIndex:window.start.value,show:window.show.value,fileActions:true,downloadUrl:()=>url,fetchBlobUrl:async()=>url,streamUrl:async()=>window.testVideoUrl,onClose:()=>window.show.value=false})}).mount('#fixture');
   </script>`}));
   await page.goto(base+'/lightbox-fixture');
   await expect(page.locator('.lightbox-media-img')).toBeVisible();
   await page.waitForTimeout(200);
   const boxes=await page.locator('.lightbox-btn,.lightbox-nav,.file-actions-trigger').evaluateAll(es=>es.map(e=>{const r=e.getBoundingClientRect();return {name:e.getAttribute('aria-label'),x:r.x,y:r.y,w:r.width,h:r.height}}));
   for(const a of boxes){if(a.w<44||a.h<44||a.x<0||a.x+a.w>width+.1||a.y<28||a.y+a.h>height-24+.1)throw Error('Unsafe control '+JSON.stringify(a));}
   for(let i=0;i<boxes.length;i++)for(let j=i+1;j<boxes.length;j++){const a=boxes[i],b=boxes[j];if(a.x<b.x+b.w&&a.x+a.w>b.x&&a.y<b.y+b.h&&a.y+a.h>b.y)throw Error('Overlapping controls');}
   const title=await page.locator('.lightbox-title').boundingBox();if(title.y<28)throw Error('Title overlaps system bar');
   await page.getByRole('button',{name:'Fullscreen',exact:true}).click();
   await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(true);
   await page.locator('.file-actions-trigger').click();await expect(page.getByRole('link',{name:'Download',exact:true})).toBeVisible();await page.getByRole('button',{name:'Close menu',exact:true}).click();
   await page.getByRole('button',{name:'Exit fullscreen',exact:true}).click();
   await expect.poll(()=>page.evaluate(()=>!!document.fullscreenElement)).toBe(false);
   // A short local landscape video exercises native controls without fetching a user file.
   await page.evaluate(async()=>{const c=document.createElement('canvas');c.width=640;c.height=360;const g=c.getContext('2d');g.fillStyle='#234';g.fillRect(0,0,640,360);const stream=c.captureStream(10);const r=new MediaRecorder(stream,{mimeType:'video/webm'});const chunks=[];r.ondataavailable=e=>chunks.push(e.data);const done=new Promise(resolve=>r.onstop=resolve);r.start();for(let i=0;i<12;i++){g.fillStyle=i%2?'#234':'#345';g.fillRect(0,0,640,360);await new Promise(resolve=>setTimeout(resolve,50));}r.stop();await done;stream.getTracks().forEach(t=>t.stop());window.testVideoUrl=URL.createObjectURL(new Blob(chunks,{type:'video/webm'}));});
   await page.getByRole('button',{name:'Previous file',exact:true}).click();
   const video=page.locator('video');await expect(video).toBeVisible();
   await expect.poll(()=>video.evaluate(v=>v.readyState)).toBeGreaterThanOrEqual(2);await video.evaluate(v=>{v.loop=true;return v.play()});await expect.poll(()=>video.evaluate(v=>v.currentTime)).toBeGreaterThan(0);
   const media=await video.boundingBox();if(media.height>height-110||media.width>width||await video.evaluate(v=>getComputedStyle(v).objectFit)!=='contain')throw Error('Video stretched or controls clipped '+JSON.stringify(media));
   await page.screenshot({path:'/tmp/archy-lightbox-'+width+'x'+height+'.png'});
   await page.getByRole('button',{name:'Close viewer',exact:true}).click();await expect(page.getByRole('dialog')).toHaveCount(0);
   if(errors.length)throw Error(errors.join('\n'));
   console.log('PASS',width,height,'safe areas, 44px nonoverlapping controls, image fullscreen/exit, real landscape video, close');
   await context.close();
  }
 } finally {await browser.close()}
})().catch(e=>{console.error(e);process.exit(1)});
