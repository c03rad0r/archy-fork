export type ContentTab = 'film' | 'song' | 'podcast' | 'book' | 'tvshow' | 'image' | 'place' | 'recipe' | 'news' | 'websites' | 'magazine' | 'code' | 'design-system' | 'nostr' | 'app' | 'favorites' | 'discover' | 'prompt'

export interface MagazineSection {
  title: string
  content: string
  imageUrl?: string
  author?: string
  url?: string
  group?: string
}

// ─── Query classifiers ───────────────────────────────────────────

export function isNewsQuery(q: string): boolean {
  const lower = q.toLowerCase().trim()
  if (!lower) return false
  return /\b(news|latest|recent|current|what'?s happening|updates? about|headlines?|breaking|press|media coverage)\b/.test(lower) ||
    /what'?s the latest|latest \w+ news/.test(lower) ||
    /what are people saying|what'?s the word|what do people think/.test(lower) ||
    /what happened (today|this week|recently|yesterday)|any updates? on|what'?s (new|going on)|trending|in the news/i.test(lower) ||
    /current events|today'?s top|catch me up|brief me|fill me in/i.test(lower)
}

export function isNewsLikeResponse(text: string): boolean {
  const lower = text.toLowerCase()
  return /for instant .* news|check these sources|for (the )?latest (bitcoin )?news|direct sources/i.test(lower) ||
    /(have )?access to (live )?web search|want me to (go back and )?search/i.test(lower) ||
    /i can'?t access the web.{0,30}(but|however)|having trouble reaching the web.{0,30}(but|however)|unable to browse.{0,30}(but|however)|can'?t search the web.{0,30}(but|however)/i.test(lower) ||
    /here are.{0,20}(reliable|trusted|good) sources|top sources for/i.test(lower)
}

export function isMusicQuery(q: string): boolean {
  if (!q) return false
  return /\b(song|songs|music|track|tracks|playlist|playlists|album|albums|listen|listening|sing|singing|singer|singers|band|bands|artist|artists|rapper|rappers|rap|hip hop|r&b|rock|jazz|classical|edm|electronic|pop music|concert|concerts|vinyl|soundtrack|anthem|beat|beats|melody|melodies|tune|tunes|lyric|lyrics|acoustic|remix|dj|genre|genres|spotify|soundcloud|bandcamp|musician|musicians|composer|composers|orchestra|symphony|punk|metal|reggae|blues|soul|funk|country music|grammys?|billboard|top 40|mixtape|ep\b|lp\b|discography|jam|jams|banger|bangers)\b/i.test(q) ||
    /recommend.*(song|music|track|listen)/i.test(q) ||
    /play\s+(me\s+)?(some|a)\b/i.test(q) ||
    /what genre|favorite (song|music|band|artist|jam|tune)|best (song|album|track|music)/i.test(q)
}

export function isWebsitesQuery(q: string): boolean {
  const lower = q.toLowerCase().trim()
  return /\b(website|websites|where to check|best places? to check|places? to (look|check|find)|check online|resources?|sources? to (check|read|visit)|links?|urls?|sites?|portals?|platforms? for|tools? for|apps? for|services? for)\b/.test(lower) ||
    /where (can i|should i) (check|look|find|go|visit|browse)/.test(lower) ||
    /point me to|direct me to|link me|send me (to|a link|some links)|any good (sites|resources|tools|platforms)/i.test(lower)
}

export function isWebsitesLikeResponse(text: string): boolean {
  const lower = text.toLowerCase()
  return /best places? to check|check online yourself|places? to check online|websites? to (visit|check|read)/i.test(lower) ||
    /here are (some|a few|the best) (resources|websites|sites|links|tools|platforms)/i.test(lower) ||
    /i'?d recommend (checking|visiting|looking at)|you (can|could|should|might) (check|visit|try|look at|browse)/i.test(lower) ||
    /useful (resources|websites|sites|links|tools)|helpful (resources|websites|sites|links)/i.test(lower)
}

export function isBookQuery(q: string): boolean {
  return /\b(book|books|read|reading|novel|novels|author|authors|nonfiction|non-fiction|recommend.*read|must.read|literature|memoir|memoirs|biography|biographies|autobiography|paperback|hardcover|kindle|audible|audiobook|audiobooks|bookshelf|bestseller|bestsellers|goodreads|epub)\b/i.test(q) ||
    /what should i read|favorite reads?|reading list|book club|book recommendation|suggest.*book|what.*worth reading|good reads?|anything to read|currently reading/i.test(q)
}

export function isBookLikeResponse(text: string): boolean {
  return /\b(novel|author|pages?|ISBN|published|bestsell|literary|fiction|nonfiction|book)\b/i.test(text) &&
    (text.match(/\bby\s+[A-Z]/g)?.length ?? 0) >= 1
}

export function isTVQuery(q: string): boolean {
  return /\b(tv\b|tv shows?|tv series|series|television|streaming|binge|watch|recommend.*shows?|best shows?|seasons?|netflix|hbo|hulu|disney\+?|apple tv|amazon prime|peacock|paramount\+?|showtime|miniseries|docuseries|sitcom|drama series|limited series|pilot|showrunner|renewed|cancelled|premiere)\b/i.test(q) ||
    /what'?s good on|anything to (binge|watch)|what should (i|we) (watch|stream)|good (shows?|series) to|new (shows?|series)|best (shows?|series)|recommend.*(shows?|series|watch)/i.test(q)
}

export function isImageQuery(q: string): boolean {
  return /\b(image|images|photo|photos|picture|pictures|screenshot|screenshots|gallery|artwork|illustration|visual|infographic|diagram|chart)\b/i.test(q)
}

export function isPlaceQuery(q: string): boolean {
  return /\b(restaurant|restaurants|place|places|food|eat|eating|dining|cafe|cafes|bar|bars|pub|pubs|brunch|lunch|dinner|bistro|pizza|pizzeria|sushi|ramen|tacos|burger|bakery|deli|steakhouse|where to eat|good food|best food|where should i eat|recommend.*eat|recommend.*restaurant|recommend.*place|hungry|starving|takeout|take-?out|delivery|reservation|reservations|michelin|yelp|zagat|foodie|gastropub|tapas|dim sum|bbq|barbecue|food truck|brewery|winery|cocktail bar|speakeasy|rooftop bar|happy hour)\b/i.test(q) ||
    /where.*(eat|food|drink|grab|dine)|best.*(brunch|lunch|dinner|food|restaurant|eat|spot)|good.*(food|restaurant|eat|spot)|what'?s good to eat/i.test(q)
}

export function isPlaceLikeResponse(text: string): boolean {
  return /\b(restaurant|cuisine|menu|reserv|dining|address|open|hours|price range|\$\$|\$\$\$|michelin|yelp|rating)\b/i.test(text) &&
    (text.match(/\b(?:restaurant|cafe|bar|bistro|pub|pizzeria|bakery|deli|steakhouse|grill)\b/gi)?.length ?? 0) >= 2
}

// ─── Recipe classifiers ─────────────────────────────────────────

export function isRecipeQuery(q: string): boolean {
  if (!q) return false
  return /\b(recipe|recipes|cook|cooking|bake|baking|meal|meals|dish|dishes|ingredient|ingredients|how to make|how to cook|how to bake)\b/i.test(q) ||
    /make me a|cook me|bake me|recipe for|what can i (make|cook|bake)|meal prep|meal plan/i.test(q)
}

export function isRecipeLikeResponse(text: string): boolean {
  return /<recipe_ext\s/i.test(text)
}

// ─── Code classifiers ───────────────────────────────────────────

export function isCodeQuery(q: string): boolean {
  if (!q) return false
  return /\b(code|coding|programming|function|algorithm|implement|debug|syntax|API|library|framework|snippet|script|regex|refactor|compile|runtime|variable|class|method|module|package|dependency|typescript|javascript|python|rust|go|java|swift|kotlin|ruby|php|css|html|sql|bash|shell|cli|terminal|git|docker|webpack|vite|npm|yarn|pnpm)\b/i.test(q) ||
    /write (me )?a |how (do i|to) (code|program|implement|build|create|make)|show me.*code|code (example|sample)|fix.*(bug|error|issue)|what does this code/i.test(q)
}

export function isCodeLikeResponse(text: string): boolean {
  const codeBlockCount = (text.match(/```[\s\S]*?```/g) ?? []).length
  return codeBlockCount >= 3
}

// ─── Nostr classifiers ──────────────────────────────────────────

export function isNostrQuery(q: string): boolean {
  if (!q) return false
  return /\b(nostr|npub[a-z0-9]{8,}|nip-?\d+|damus|primal|snort|amethyst|coracle|iris|nos\.social|zaps?\b|relays?\b|naddr|nevent|nprofile|note1[a-z0-9]+|nostrich|fiatjaf)\b/i.test(q) ||
    /\b(social media|social network|social protocol)\b.*\b(decentrali|censorship|relay|open)\b/i.test(q) ||
    /decentralized social|censorship.resistant.*(social|network|protocol)/i.test(q)
}

export function isNostrLikeResponse(text: string): boolean {
  // Literal "nostr" is always sufficient
  if (/\bnostr\b/i.test(text)) return true
  // Otherwise require 2+ distinct Nostr-specific signals
  const signals = [
    /\bnpub[a-z0-9]{8,}\b/i,
    /\bnip-?\d+\b/i,
    /\b(damus|primal|snort|amethyst|coracle|iris|nos\.social|nostrudel)\b/i,
    /\b(relay|relays)\b.*\b(wss?:\/\/|connect|publish)\b/i,
    /\bzaps?\b.*\b(lightning|sats|send)\b/i,
    /\bnote1[a-z0-9]+\b/i,
    /\bnevent[a-z0-9]+\b/i,
    /\bnprofile[a-z0-9]+\b/i,
    /\bnostrich\b/i,
    /\bfiatjaf\b/i,
  ]
  return signals.filter(re => re.test(text)).length >= 2
}

// ─── App classifiers ────────────────────────────────────────────

export function isAppQuery(q: string): boolean {
  if (!q) return false
  return /\b(app|apps|application|applications|client|clients|wallet|wallets|tool|tools|software|download|install)\b/i.test(q) &&
    /\b(best|good|recommend|suggest|which|what|top|favorite|popular|use|try|need)\b/i.test(q) ||
    /what app|which app|best app|recommend.*app|suggest.*app|best.*client|best.*wallet|recommend.*wallet|recommend.*tool/i.test(q) ||
    /what.*(use|download|install) for|how (do i|to) (use|get|install|set up)/i.test(q)
}

export function isAppLikeResponse(text: string): boolean {
  const lower = text.toLowerCase()
  const appSignals = [
    /popular (clients?|apps?|wallets?|tools?) include/i,
    /you (can|could|might|should) (use|try|check out|download|install)/i,
    /available (on|for) (ios|android|web|desktop|mac|windows|linux)/i,
    /download (from|on|at)/i,
    /(app store|play store|google play|f-?droid|github releases?)/i,
    /open.?source.*(app|client|tool|wallet)/i,
  ]
  return appSignals.filter(re => re.test(lower)).length >= 2
}

// ─── Tab filtering ────────────────────────────────────────────────

export function extractQueryContext(q: string): string {
  const stop = /\b(what|is|are|the|a|an|latest|recent|current|news|about|for|how|why|when|where|can|could|should|would|tell|me|please|best|good)\b/gi
  const cleaned = q.replace(stop, ' ').replace(/\s+/g, ' ').trim().slice(0, 60)
  return cleaned || ''
}

export function preferredFirstTab(userQuery: string): ContentTab | null {
  const q = userQuery.toLowerCase().trim()
  // `films` (plural) was missing: "recommend me 10 scifi films" — the
  // operator's own words — matched nothing and opened no content tab.
  if (/\b(film|films|movie|movies)\b/.test(q)) return 'film'
  // Podcast is checked BEFORE song because its words are the specific
  // ones: "listen to a podcast" must not be swallowed by the song rule's
  // bare `listen`. Conversely `show` is NOT a podcast word — it is how
  // operators phrase almost every request ("show me my files"), and it
  // made unrelated queries render "Podcast recommendations". `tv show`
  // keeps its own two-word form in the tvshow rule below.
  if (/\b(podcast|episode)\b/.test(q)) return 'podcast'
  if (/\b(song|music|track|album|band|artist|listen)\b/.test(q)) return 'song'
  if (/\b(book|books|read|reading|novel|author|nonfiction|non-fiction)\b/.test(q)) return 'book'
  if (/\b(tv\b|tv show|tv series|series|television|streaming|binge|watch)\b/.test(q)) return 'tvshow'
  if (/\b(image|images|photo|photos|picture|pictures|screenshot|gallery|artwork|illustration)\b/.test(q)) return 'image'
  if (/\b(restaurant|restaurants|place|places|food|eat|dining|cafe|cafes|bar|bars|pub|pubs|brunch|lunch|dinner|bistro|pizza|pizzeria|sushi|ramen|tacos|burger|bakery|deli|steakhouse|hungry)\b/.test(q)) return 'place'
  if (isRecipeQuery(q)) return 'recipe'
  if (isCodeQuery(q)) return 'code'
  if (isAppQuery(q)) return 'app'
  if (isNostrQuery(q)) return 'nostr'
  if (isNewsQuery(q)) return 'news'
  if (isWebsitesQuery(q)) return 'websites'
  return null
}

export function filterTabsByContext(
  userQuery: string,
  hasFilms: boolean,
  hasSongs: boolean,
  hasPodcasts: boolean,
  hasBooks: boolean,
  hasTVSeries: boolean,
  hasImages: boolean,
  hasPlaces: boolean,
  hasNews: boolean,
  hasWebsites: boolean,
  hasMagazine: boolean,
  hasNostr: boolean,
  hasApps: boolean,
  hasCode = false,
  hasRecipes = false,
): ContentTab[] {
  const q = userQuery.toLowerCase().trim()
  const preferred = preferredFirstTab(userQuery)

  // Build the full set of detected content tabs
  const all: ContentTab[] = []
  if (hasFilms) all.push('film')
  if (hasBooks) all.push('book')
  if (hasTVSeries) all.push('tvshow')
  if (hasImages) all.push('image')
  if (hasPlaces) all.push('place')
  if (hasRecipes) all.push('recipe')
  if (hasSongs) all.push('song')
  if (hasPodcasts) all.push('podcast')
  if (hasCode) all.push('code')
  if (hasApps) all.push('app')
  if (hasMagazine) all.push('magazine')
  if (hasNews) all.push('news')
  if (hasWebsites) all.push('websites')
  if (hasNostr) all.push('nostr')

  // Helper: prioritize certain tabs first, then append remaining detected content
  const prioritize = (priority: ContentTab[]): ContentTab[] => {
    const present = priority.filter(t => all.includes(t))
    const rest = all.filter(t => !present.includes(t))
    return [...present, ...rest]
  }

  // Nostr query → prioritize nostr tab
  if (isNostrQuery(q)) {
    return prioritize(['nostr', 'app', 'magazine', 'websites'])
  }

  // App query → prioritize apps tab
  if (isAppQuery(q)) {
    const result = prioritize(['app', 'nostr', 'magazine', 'websites'])
    return result.length > 0 ? result : hasNostr ? ['nostr'] : []
  }

  // News query → prioritize news/magazine tabs
  if (isNewsQuery(q)) {
    return prioritize(['magazine', 'news', 'websites', 'podcast'])
  }

  if (hasMagazine && all.length === 1) {
    return ['magazine']
  }

  if (hasWebsites && all.length === 1) {
    return ['websites']
  }

  if (preferred && all.includes(preferred)) {
    const rest = all.filter((t) => t !== preferred)
    return [preferred, ...rest]
  }
  return all
}
