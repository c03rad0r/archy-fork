import type { ImageItem } from '@aiui/core/types/content'

export const mockImages: ImageItem[] = [
  { id: 'img1', title: 'Cyberpunk City at Night', url: 'https://picsum.photos/id/1/1200/800', width: 1200, height: 800, alt: 'Cyberpunk city at night' },
  { id: 'img2', title: 'Mountain Landscape', url: 'https://picsum.photos/id/10/1200/800', width: 1200, height: 800, alt: 'Mountain landscape' },
  { id: 'img3', title: 'Ocean Sunset', url: 'https://picsum.photos/id/14/1200/800', width: 1200, height: 800, alt: 'Ocean sunset' },
  { id: 'img4', title: 'Forest Path', url: 'https://picsum.photos/id/15/1200/800', width: 1200, height: 800, alt: 'Forest path' },
  { id: 'img5', title: 'Abstract Art', url: 'https://picsum.photos/id/20/1200/800', width: 1200, height: 800, alt: 'Abstract art' },
  { id: 'img6', title: 'Desert Dunes', url: 'https://picsum.photos/id/22/1200/800', width: 1200, height: 800, alt: 'Desert dunes' },
  { id: 'img7', title: 'City Architecture', url: 'https://picsum.photos/id/26/1200/800', width: 1200, height: 800, alt: 'City architecture' },
  { id: 'img8', title: 'Northern Lights', url: 'https://picsum.photos/id/29/1200/800', width: 1200, height: 800, alt: 'Northern lights' },
  { id: 'img9', title: 'Space Nebula', url: 'https://picsum.photos/id/30/1200/800', width: 1200, height: 800, alt: 'Space nebula' },
  { id: 'img10', title: 'Macro Photography', url: 'https://picsum.photos/id/36/1200/800', width: 1200, height: 800, alt: 'Macro photography' },
  { id: 'img11', title: 'Winter Scene', url: 'https://picsum.photos/id/40/1200/800', width: 1200, height: 800, alt: 'Winter scene' },
  { id: 'img12', title: 'Street Photography', url: 'https://picsum.photos/id/42/1200/800', width: 1200, height: 800, alt: 'Street photography' },
  { id: 'img13', title: 'Underwater World', url: 'https://picsum.photos/id/45/1200/800', width: 1200, height: 800, alt: 'Underwater world' },
  { id: 'img14', title: 'Vintage Train', url: 'https://picsum.photos/id/47/1200/800', width: 1200, height: 800, alt: 'Vintage train' },
  { id: 'img15', title: 'Botanical Garden', url: 'https://picsum.photos/id/49/1200/800', width: 1200, height: 800, alt: 'Botanical garden' },
  { id: 'img16', title: 'Aerial View', url: 'https://picsum.photos/id/50/1200/800', width: 1200, height: 800, alt: 'Aerial view' },
  { id: 'img17', title: 'Neon Signs', url: 'https://picsum.photos/id/55/1200/800', width: 1200, height: 800, alt: 'Neon signs' },
  { id: 'img18', title: 'Foggy Bridge', url: 'https://picsum.photos/id/57/1200/800', width: 1200, height: 800, alt: 'Foggy bridge' },
  { id: 'img19', title: 'Lightning Storm', url: 'https://picsum.photos/id/60/1200/800', width: 1200, height: 800, alt: 'Lightning storm' },
  { id: 'img20', title: 'Retro Computer', url: 'https://picsum.photos/id/63/1200/800', width: 1200, height: 800, alt: 'Retro computer' },
  { id: 'img21', title: 'Crystal Cave', url: 'https://picsum.photos/id/65/1200/800', width: 1200, height: 800, alt: 'Crystal cave' },
]

export function searchImages(query: string): ImageItem[] {
  const q = query.toLowerCase()
  return mockImages.filter((img) => (img.title ?? '').toLowerCase().includes(q))
}
