# Keep WebView JavaScript interface
-keepclassmembers class com.archipelago.app.ui.screens.WebViewScreen$* {
    public *;
}

# Keep Compose
-dontwarn androidx.compose.**
