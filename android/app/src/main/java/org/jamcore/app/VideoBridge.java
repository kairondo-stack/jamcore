package org.jamcore.app;

import android.media.Image;
import android.media.MediaCodec;
import android.media.MediaExtractor;
import android.media.MediaFormat;
import android.util.Log;

import java.nio.ByteBuffer;

/**
 * Videodateien als Bildquelle fuer die NativeActivity.
 *
 * MediaExtractor/MediaCodec sind Java-APIs (MediaCodec braucht Callbacks
 * bzw. einen Dekodier-Thread). Diese Klasse dekodiert synchron in einen
 * ByteBuffer, zieht die drei YUV-Planes aus dem Ausgabebild und schiebt sie
 * ueber dieselbe native Methode wie die Kamera (jamcore::yuv macht RGBA).
 *
 * Die Datei muss fuer die App lesbar sein (App-Ordner videos/), sonst
 * scheitert MediaExtractor.setDataSource.
 */
public final class VideoBridge {
    private static final String TAG = "jamandroid::video";

    static {
        System.loadLibrary("jamandroid");
    }

    private static native void nativeVideoFrame(
            byte[] y, int yRowStride, int yPixelStride,
            byte[] u, int uRowStride, int uPixelStride,
            byte[] v, int vRowStride, int vPixelStride,
            int width, int height);

    private static MediaExtractor extractor;
    private static MediaCodec codec;
    private static Thread thread;
    private static volatile boolean running;
    private static volatile int fps = 20;
    private static String fehler = "";

    /** Startet die Wiedergabe der Datei; Rueckgabe "" oder Fehlertext. */
    public static synchronized String start(String pfad, int fpsWunsch) {
        stop();
        fehler = "";
        fps = Math.max(1, Math.min(60, fpsWunsch));
        try {
            extractor = new MediaExtractor();
            extractor.setDataSource(pfad);
            int spur = -1;
            MediaFormat format = null;
            for (int i = 0; i < extractor.getTrackCount(); i++) {
                MediaFormat f = extractor.getTrackFormat(i);
                String mime = f.getString(MediaFormat.KEY_MIME);
                if (mime != null && mime.startsWith("video/")) {
                    spur = i;
                    format = f;
                    break;
                }
            }
            if (spur < 0 || format == null) {
                fehler = "keine Videospur";
                return fehler;
            }
            extractor.selectTrack(spur);
            String mime = format.getString(MediaFormat.KEY_MIME);
            codec = MediaCodec.createDecoderByType(mime);
            codec.configure(format, null, null, 0);
            codec.start();
            running = true;
            thread = new Thread(VideoBridge::schleife, "jam-video");
            thread.start();
            Log.i(TAG, "start " + pfad + " (" + mime + ", " + fps + " fps)");
        } catch (Throwable e) {
            fehler = e.getClass().getSimpleName() + ": " + e.getMessage();
            Log.i(TAG, "start: " + fehler);
        }
        return fehler;
    }

    private static void schleife() {
        MediaCodec.BufferInfo info = new MediaCodec.BufferInfo();
        while (running) {
            try {
                int inIdx = codec.dequeueInputBuffer(10_000);
                if (inIdx >= 0) {
                    ByteBuffer puffer = codec.getInputBuffer(inIdx);
                    int groesse = puffer == null ? -1 : extractor.readSampleData(puffer, 0);
                    if (groesse < 0) {
                        codec.queueInputBuffer(
                                inIdx, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM);
                    } else {
                        codec.queueInputBuffer(inIdx, 0, groesse, extractor.getSampleTime(), 0);
                        extractor.advance();
                    }
                }
                int outIdx = codec.dequeueOutputBuffer(info, 10_000);
                if (outIdx >= 0) {
                    boolean ende = (info.flags & MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0;
                    Image bild = codec.getOutputImage(outIdx);
                    if (bild != null) {
                        Image.Plane[] planes = bild.getPlanes();
                        nativeVideoFrame(
                                kopie(planes[0]), planes[0].getRowStride(),
                                planes[0].getPixelStride(),
                                kopie(planes[1]), planes[1].getRowStride(),
                                planes[1].getPixelStride(),
                                kopie(planes[2]), planes[2].getRowStride(),
                                planes[2].getPixelStride(),
                                bild.getWidth(), bild.getHeight());
                        bild.close();
                    }
                    codec.releaseOutputBuffer(outIdx, false);
                    if (ende) {
                        // Endlosschleife wie der Desktop-Player.
                        extractor.seekTo(0, MediaExtractor.SEEK_TO_CLOSEST_SYNC);
                        codec.flush();
                    }
                    Thread.sleep(Math.max(1, 1000 / fps));
                }
            } catch (Throwable e) {
                fehler = e.getClass().getSimpleName() + ": " + e.getMessage();
                Log.i(TAG, "Schleife: " + fehler);
                running = false;
            }
        }
        Log.i(TAG, "Dekodier-Schleife beendet");
    }

    private static byte[] kopie(Image.Plane plane) {
        ByteBuffer puffer = plane.getBuffer();
        byte[] bytes = new byte[puffer.remaining()];
        puffer.get(bytes);
        return bytes;
    }

    public static synchronized void stop() {
        running = false;
        Thread t = thread;
        thread = null;
        if (t != null) {
            try {
                t.join(500);
            } catch (InterruptedException ignored) {
            }
        }
        try {
            if (codec != null) {
                codec.stop();
                codec.release();
                codec = null;
            }
        } catch (Throwable ignored) {
        }
        try {
            if (extractor != null) {
                extractor.release();
                extractor = null;
            }
        } catch (Throwable ignored) {
        }
        Log.i(TAG, "gestoppt");
    }

    public static synchronized void setFps(int neu) {
        fps = Math.max(1, Math.min(60, neu));
    }

    public static synchronized String status() {
        return fehler;
    }
}
