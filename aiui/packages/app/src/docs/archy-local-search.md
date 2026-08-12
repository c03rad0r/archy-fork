# Archy Local Search — Content Surface Guide

## How It Works

When Archy (the AI) responds to a query, AIUI's **content extraction pipeline** scans the response text for structured tags and patterns. Each detected content type is routed to the appropriate **content surface** (grid component) in the side panel.

The **ContextBroker** (in `contentFiltering.ts`) classifies the user query to determine which tabs to show and in what order.

## Content Type → Surface Mapping

| Content Type | Extraction Source | Surface (Grid) |
|-------------|-------------------|----------------|
| Films | `[[film_ext:Title\|Year\|Director]]` tags | FilmGrid |
| Songs | `[[song_ext:Title\|Artist]]` tags, `"Title" by Artist` patterns | SongGrid |
| Podcasts | `[[podcast_ext:Title\|Host]]` tags | PodcastGrid |
| Books | `[[book_ext:Title\|Author]]` tags, `**Title** by Author` patterns | BookGrid |
| TV Series | `[[tv_ext:Title\|Year\|Creator]]` tags | TVSeriesGrid |
| Images | Markdown `![alt](url)` or raw image URLs | ImageGrid |
| Places | `[[place_ext:Name\|Cuisine\|City\|Rating\|Price\|Address]]` | PlaceGrid |
| Magazine | `## Heading` + bullet/numbered lists with bold titles | MagazineGrid |
| News | Web search results, markdown links | NewsGrid |
| Code | Markdown fenced code blocks (3+ triggers code tab) | Code renderer |
| Apps | App-specific keyword patterns | AppsGrid |
| Nostr | Nostr protocol terms (npub, nip, zaps) | NostrGrid |

## ContextBroker Filtering

The ContextBroker classifies queries to prioritize relevant tabs:

### Query Classifiers

- **Music query** (`song|music|track|album|artist|listen|spotify`): Prioritizes SongGrid
- **Film query** (`movie|film|cinema|watch|director`): Prioritizes FilmGrid
- **TV query** (`tv|series|netflix|season|episode|binge`): Prioritizes TVSeriesGrid
- **Book query** (`book|read|author|novel|literature`): Prioritizes BookGrid
- **News query** (`news|latest|recent|headlines|updates`): Prioritizes MagazineGrid → NewsGrid
- **Place query** (`restaurant|food|eat|cafe|bar|dining`): Prioritizes PlaceGrid
- **Code query** (code blocks in response + `code|programming|function`): Prioritizes Code tab
- **App query** (`app|install|software|tool`): Prioritizes AppsGrid
- **Nostr query** (`nostr|npub|relay|zap|lightning`): Prioritizes NostrGrid

### Tab Visibility

Tabs only appear when content is detected. The classifier determines ordering:
1. Primary tab (from query classification)
2. Secondary tabs (with detected content)
3. `prompt` tab (always last)

## File Type → Content Surface (Local Files)

When browsing local files via the file browser (`/browse`), file types map to surfaces:

| File Extension | Content Surface |
|---------------|----------------|
| `.mp3`, `.flac`, `.wav`, `.ogg`, `.m4a` | SongGrid (music playback) |
| `.jpg`, `.png`, `.gif`, `.webp`, `.svg` | ImageGrid (image gallery) |
| `.mp4`, `.mkv`, `.avi`, `.mov` | Video player |
| `.pdf` | PDF viewer |
| `.md`, `.txt` | Markdown/text renderer |
| `.ts`, `.js`, `.vue`, `.py`, `.rs`, etc. | Code viewer with syntax highlighting |
| `.json`, `.yaml`, `.toml` | Code viewer (config files) |

## Architecture

```
User Query
    ↓
AI Response (with content tags)
    ↓
contentExtraction.ts — extracts all content types
    ↓
contentFiltering.ts — classifies query, determines tab order
    ↓
useContentPanel.ts — manages panel state, active tab
    ↓
*Grid.vue components — render content in panel
```

## Adding New Content Types

1. Define the type in `@aiui/core/types/content.ts`
2. Add extraction logic in `contentExtraction.ts`
3. Add query classifier in `contentFiltering.ts`
4. Create `*Grid.vue` and `*Detail.vue` in `components/content/`
5. Register the tab in `useContentPanel.ts`
