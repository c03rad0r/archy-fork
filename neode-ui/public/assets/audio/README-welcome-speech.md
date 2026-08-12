# Welcome Noderunner Speech

The intro plays a sci-fi female voice saying "Welcome Noderunner" as the text types in.

## Generate the audio (ElevenLabs)

1. Get a free API key at [elevenlabs.io](https://elevenlabs.io) (free tier: 10k chars/month)
2. Run:
   ```bash
   cd neode-ui
   ELEVENLABS_API_KEY=your_key npm run generate-welcome-speech
   ```
3. Commit `welcome-noderunner.mp3` to the repo

## Custom sci-fi voice

Browse [ElevenLabs Voice Library](https://elevenlabs.io/voice-library) and search for "sci-fi", "AI", "robot", or "character". Copy the voice ID from the URL or voice settings, then:

```bash
ELEVENLABS_API_KEY=your_key ELEVENLABS_VOICE_ID=voice_id npm run generate-welcome-speech
```

Recommended: "The Digital Oracle", "The Friendly AI Assistant", or similar character voices from the Synthetic/Character categories.
