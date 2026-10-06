package org.jamcore.app;

import android.content.Context;
import android.net.wifi.WifiManager;
import android.util.Log;

/**
 * MulticastLock fuer Ableton Link.
 *
 * Android filtert eingehende Multicast-Pakete im WLAN weg, solange kein
 * MulticastLock gehalten wird. Ohne ihn sieht die Link-Session keine Peers.
 * Das Lock wird gehalten, solange Link aktiv ist (setReferenceCounted(false),
 * damit acquire/release nicht aufzaehlen).
 */
public final class LinkSupport {
    private static final String TAG = "jamandroid::link";

    private static WifiManager.MulticastLock lock;

    /** Holt das MulticastLock (mehrfache Aufrufe sind harmlos). */
    public static synchronized String acquire(Context context) {
        try {
            if (lock != null && lock.isHeld()) {
                return "";
            }
            WifiManager wlan =
                    (WifiManager) context.getApplicationContext().getSystemService(
                            Context.WIFI_SERVICE);
            if (wlan == null) {
                return "kein WifiManager";
            }
            lock = wlan.createMulticastLock("jamcore-link");
            lock.setReferenceCounted(false);
            lock.acquire();
            Log.i(TAG, "MulticastLock aktiv");
            return "";
        } catch (Throwable e) {
            Log.i(TAG, "MulticastLock: " + e);
            return e.getClass().getSimpleName() + ": " + e.getMessage();
        }
    }

    /** Gibt das Lock wieder her. */
    public static synchronized void release() {
        try {
            if (lock != null && lock.isHeld()) {
                lock.release();
                Log.i(TAG, "MulticastLock freigegeben");
            }
        } catch (Throwable e) {
            Log.i(TAG, "release: " + e);
        }
        lock = null;
    }
}
