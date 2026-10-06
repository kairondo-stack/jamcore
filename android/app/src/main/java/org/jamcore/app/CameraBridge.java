package org.jamcore.app;

import android.content.Context;
import android.graphics.ImageFormat;
import android.hardware.camera2.CameraCaptureSession;
import android.hardware.camera2.CameraDevice;
import android.hardware.camera2.CameraManager;
import android.hardware.camera2.CaptureRequest;
import android.media.Image;
import android.media.ImageReader;
import android.os.Handler;
import android.os.HandlerThread;
import android.util.Log;

import java.nio.ByteBuffer;
import java.util.Arrays;

/**
 * Kamera-Bruecke fuer die NativeActivity.
 *
 * Camera2 verlangt Pflicht-Callbacks (CameraDevice.StateCallback,
 * CaptureSession.StateCallback, ImageReader.OnImageAvailableListener), die
 * sich in reinem Rust/JNI nicht erzeugen lassen. Diese kleine Java-Klasse
 * haelt Kamera, Session und ImageReader und schiebt jeden Frame ueber die
 * native Methode weiter; die Umrechnung nach RGBA macht jamcore.
 */
public final class CameraBridge {
    private static final String TAG = "jamandroid::camera";

    private static CameraDevice geraet;
    private static CameraCaptureSession session;
    private static ImageReader reader;
    private static HandlerThread thread;
    private static Handler handler;
    private static String fehler = "";

    static {
        System.loadLibrary("jamandroid");
    }

    private static native void nativeFrame(
            byte[] y, int yRowStride, int yPixelStride,
            byte[] u, int uRowStride, int uPixelStride,
            byte[] v, int vRowStride, int vPixelStride,
            int width, int height);

    /** Startet die Vorschau der ersten Kamera; Rueckgabe "" oder Fehlertext. */
    public static synchronized String start(Context context, int width, int height) {
        if (geraet != null) {
            return "";
        }
        fehler = "";
        try {
            thread = new HandlerThread("jam-camera");
            thread.start();
            handler = new Handler(thread.getLooper());

            CameraManager manager =
                    (CameraManager) context.getSystemService(Context.CAMERA_SERVICE);
            String id = manager.getCameraIdList()[0];
            reader = ImageReader.newInstance(width, height, ImageFormat.YUV_420_888, 2);
            reader.setOnImageAvailableListener(CameraBridge::onImage, handler);
            manager.openCamera(id, new CameraDevice.StateCallback() {
                @Override
                public void onOpened(CameraDevice camera) {
                    geraet = camera;
                    startSession();
                }

                @Override
                public void onDisconnected(CameraDevice camera) {
                    camera.close();
                    geraet = null;
                    fehler = "Kamera getrennt";
                    Log.i(TAG, fehler);
                }

                @Override
                public void onError(CameraDevice camera, int error) {
                    camera.close();
                    geraet = null;
                    fehler = "Kamera-Fehler " + error;
                    Log.i(TAG, fehler);
                }
            }, handler);
        } catch (Exception e) {
            fehler = e.getClass().getSimpleName() + ": " + e.getMessage();
            Log.i(TAG, "start: " + fehler);
            return fehler;
        }
        Log.i(TAG, "start angestossen (" + width + "x" + height + ")");
        return "";
    }

    private static void startSession() {
        try {
            geraet.createCaptureSession(Arrays.asList(reader.getSurface()),
                    new CameraCaptureSession.StateCallback() {
                        @Override
                        public void onConfigured(CameraCaptureSession s) {
                            session = s;
                            try {
                                CaptureRequest.Builder b = geraet.createCaptureRequest(
                                        CameraDevice.TEMPLATE_PREVIEW);
                                b.addTarget(reader.getSurface());
                                b.set(CaptureRequest.CONTROL_MODE,
                                        CaptureRequest.CONTROL_MODE_AUTO);
                                s.setRepeatingRequest(b.build(), null, handler);
                                Log.i(TAG, "Vorschau laeuft");
                            } catch (Exception e) {
                                fehler = "Request: " + e.getMessage();
                                Log.i(TAG, fehler);
                            }
                        }

                        @Override
                        public void onConfigureFailed(CameraCaptureSession s) {
                            fehler = "Session fehlgeschlagen";
                            Log.i(TAG, fehler);
                        }
                    }, handler);
        } catch (Exception e) {
            fehler = "Session: " + e.getMessage();
            Log.i(TAG, fehler);
        }
    }

    private static void onImage(ImageReader r) {
        Image bild;
        try {
            bild = r.acquireLatestImage();
        } catch (IllegalStateException e) {
            // Reader wurde waehrend des Stopps geschlossen; kein Grund zum Absturz.
            return;
        }
        if (bild == null) {
            return;
        }
        try {
            Image.Plane[] planes = bild.getPlanes();
            nativeFrame(
                    kopie(planes[0]), planes[0].getRowStride(), planes[0].getPixelStride(),
                    kopie(planes[1]), planes[1].getRowStride(), planes[1].getPixelStride(),
                    kopie(planes[2]), planes[2].getRowStride(), planes[2].getPixelStride(),
                    bild.getWidth(), bild.getHeight());
        } catch (Exception e) {
            Log.i(TAG, "Frame: " + e);
        } finally {
            bild.close();
        }
    }

    private static byte[] kopie(Image.Plane plane) {
        ByteBuffer puffer = plane.getBuffer();
        byte[] bytes = new byte[puffer.remaining()];
        puffer.get(bytes);
        return bytes;
    }

    /** Beendet Vorschau und Kamera. */
    public static synchronized void stop() {
        try {
            if (session != null) {
                session.close();
                session = null;
            }
            // Erst den Listener abmelden, dann schliessen: sonst kann ein schon
            // eingereihter Callback auf einen geschlossenen Reader treffen.
            if (reader != null) {
                reader.setOnImageAvailableListener(null, null);
                reader.close();
                reader = null;
            }
            if (geraet != null) {
                geraet.close();
                geraet = null;
            }
            if (thread != null) {
                thread.quitSafely();
                thread = null;
            }
        } catch (Exception e) {
            Log.i(TAG, "stop: " + e.getMessage());
        }
        Log.i(TAG, "gestoppt");
    }

    /** Letzter Fehler beim Start (leer, wenn keiner). */
    public static synchronized String status() {
        return fehler;
    }
}
