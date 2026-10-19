package org.jamcore.app;

import android.content.Context;
import android.graphics.ImageFormat;
import android.hardware.camera2.CameraCaptureSession;
import android.hardware.camera2.CameraCharacteristics;
import android.hardware.camera2.CameraDevice;
import android.hardware.camera2.CameraManager;
import android.hardware.camera2.CaptureRequest;
import android.media.Image;
import android.media.ImageReader;
import android.os.Handler;
import android.os.HandlerThread;
import android.util.Log;
import android.view.Display;
import android.view.Surface;
import android.view.WindowManager;

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
    /** Activity fuer die Display-Drehung (nur waehrend die Kamera laeuft). */
    private static Context kontext;
    /** SENSOR_ORIENTATION der geoeffneten Kamera (typisch 90). */
    private static int sensorOrientierung;
    private static boolean frontKamera;
    /** Zuletzt geloggter Winkel, damit die Zeile nicht je Frame kommt. */
    private static int letzteDrehung = -1;

    static {
        System.loadLibrary("jamandroid");
    }

    private static native void nativeFrame(
            byte[] y, int yRowStride, int yPixelStride,
            byte[] u, int uRowStride, int uPixelStride,
            byte[] v, int vRowStride, int vPixelStride,
            int width, int height, int rotation);

    /** Startet die Vorschau (bevorzugt Rueckkamera); Rueckgabe "" oder Fehlertext. */
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
            String id = kamera_waehlen(manager);
            // Sensor-Orientierung und Blickrichtung fuer die Drehung: das
            // rohe Kamerabild kommt in Sensor-Koordinaten, nicht aufrecht.
            CameraCharacteristics eigenschaften = manager.getCameraCharacteristics(id);
            Integer sensor = eigenschaften.get(CameraCharacteristics.SENSOR_ORIENTATION);
            sensorOrientierung = sensor == null ? 0 : sensor;
            Integer facing = eigenschaften.get(CameraCharacteristics.LENS_FACING);
            frontKamera = facing != null && facing == CameraCharacteristics.LENS_FACING_FRONT;
            kontext = context;
            letzteDrehung = -1;
            Log.i(TAG, "sensor=" + sensorOrientierung + " display=" + displayGrad()
                    + " rotation=" + drehung() + (frontKamera ? " (Front)" : " (Rueck)"));
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
            // Der Winkel kann sich waehrend der Vorschau aendern (das Geraet
            // dreht sich, fullSensor) — je Frame neu rechnen.
            int winkel = drehung();
            if (winkel != letzteDrehung) {
                letzteDrehung = winkel;
                Log.i(TAG, "sensor=" + sensorOrientierung + " display=" + displayGrad()
                        + " rotation=" + winkel);
            }
            Image.Plane[] planes = bild.getPlanes();
            nativeFrame(
                    kopie(planes[0]), planes[0].getRowStride(), planes[0].getPixelStride(),
                    kopie(planes[1]), planes[1].getRowStride(), planes[1].getPixelStride(),
                    kopie(planes[2]), planes[2].getRowStride(), planes[2].getPixelStride(),
                    bild.getWidth(), bild.getHeight(), winkel);
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

    /**
     * Kamera fuer die Bildquelle waehlen: die erste Rueckkamera, sonst die
     * erste ueberhaupt. Auf dem Handy ist das die Hauptkamera wie bisher; auf
     * dem Emulator die Virtual-Scene statt der Front-Kamera.
     */
    private static String kamera_waehlen(CameraManager manager) throws Exception {
        String[] kennungen = manager.getCameraIdList();
        for (String kandidat : kennungen) {
            Integer blick = manager.getCameraCharacteristics(kandidat)
                    .get(CameraCharacteristics.LENS_FACING);
            if (blick != null && blick == CameraCharacteristics.LENS_FACING_BACK) {
                return kandidat;
            }
        }
        return kennungen[0];
    }

    /**
     * Drehwinkel im Uhrzeigersinn (0/90/180/270), damit das Kamerabild
     * aufrecht steht. Sensor- und Display-Drehung gehen beide ein:
     * Rueckkamera sensor - display, Frontkamera (ungespiegelte Rohdaten)
     * sensor + display — die uebliche Kamera-Rechnung.
     */
    private static int drehung() {
        int display = displayGrad();
        return frontKamera
                ? (sensorOrientierung + display) % 360
                : (sensorOrientierung - display + 360) % 360;
    }

    /** Display-Drehung als Gradzahl (ROTATION_0/90/180/270); 0 bei Fehler. */
    private static int displayGrad() {
        try {
            WindowManager fenster =
                    (WindowManager) kontext.getSystemService(Context.WINDOW_SERVICE);
            Display display = fenster.getDefaultDisplay();
            switch (display.getRotation()) {
                case Surface.ROTATION_90:
                    return 90;
                case Surface.ROTATION_180:
                    return 180;
                case Surface.ROTATION_270:
                    return 270;
                default:
                    return 0;
            }
        } catch (Exception e) {
            return 0;
        }
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
            kontext = null;
            letzteDrehung = -1;
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
