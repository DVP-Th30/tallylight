package dev.tallylight

import android.content.Context

/**
 * Initialisation Android de tallylight : charge la bibliothèque native et lui donne accès au système
 * (la bibliothèque réseau lit les serveurs DNS du réseau actif par le contexte Android).
 */
object Android {
    /**
     * À appeler une fois, avant `Tallylight.start`, par exemple dans `Application.onCreate`. Les appels
     * suivants sont sans effet. Seul le contexte de l'application est conservé : passer une activité
     * ne la retient pas.
     *
     * @throws UnsatisfiedLinkError si la bibliothèque native manque pour le processeur du téléphone.
     */
    @JvmStatic
    fun init(context: Context) {
        System.loadLibrary("tallylight_ffi")
        nativeInit(context.applicationContext ?: context)
    }

    // Côté Rust, le symbole est `Java_dev_tallylight_Android_init` : la méthode native doit donc
    // s'appeler `init` pour la JVM. Le paramètre `Any` lui donne une signature distincte de
    // `init(Context)` ci-dessus, qui garantit qu'on ne transmet que le contexte de l'application.
    @JvmStatic
    @JvmName("init")
    private external fun nativeInit(context: Any)
}
