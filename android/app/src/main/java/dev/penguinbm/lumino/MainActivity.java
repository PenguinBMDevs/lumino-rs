package dev.penguinbm.lumino;

import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.os.Build;
import android.util.Log;
import android.view.View;
import android.view.WindowManager;
import android.view.inputmethod.InputMethodManager;

import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.core.view.WindowInsetsControllerCompat;

import com.google.androidgamesdk.GameActivity;

/**
 * Lumino Android 入口 Activity。
 *
 * <p>只做三件事：加载原生库（{@code liblumino.so} → {@code android_main}）、
 * 隐藏系统栏以拿到全屏绘制区、给 Rust 侧暴露几个 JNI 可调用的平台能力。
 */
public class MainActivity extends GameActivity {

    private static final String TAG = "LuminoProbe";

    static {
        // 名字必须与 AndroidManifest 的 android.app.lib_name 及 Cargo [lib] name 一致。
        System.loadLibrary("lumino");
    }

    @Override
    protected void onCreate(android.os.Bundle savedInstanceState) {
        // 让内容延伸到系统栏之后（配合 hideSystemUI 拿满全屏）。
        WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
        hideSystemUI();
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onResume() {
        super.onResume();
        hideSystemUI();
    }

    private void hideSystemUI() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            getWindow().getAttributes().layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS;
        }

        View decorView = getWindow().getDecorView();
        WindowInsetsControllerCompat controller =
                new WindowInsetsControllerCompat(getWindow(), decorView);
        controller.hide(WindowInsetsCompat.Type.systemBars());
        controller.hide(WindowInsetsCompat.Type.displayCutout());
        controller.setSystemBarsBehavior(
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
    }

    /** JNI：唤起软键盘。 */
    private void showKeyboard() {
        Log.d(TAG, "showKeyboard");
        InputMethodManager imm = getSystemService(InputMethodManager.class);
        if (imm != null) {
            imm.showSoftInput(getWindow().getDecorView(), InputMethodManager.SHOW_IMPLICIT);
        }
    }

    /** JNI：收起软键盘。 */
    private void hideKeyboard() {
        Log.d(TAG, "hideKeyboard");
        InputMethodManager imm = getSystemService(InputMethodManager.class);
        if (imm != null) {
            imm.hideSoftInputFromWindow(getWindow().getDecorView().getWindowToken(), 0);
        }
    }

    /** JNI：读系统剪贴板文本（Android 10+ 仅前台可读）。 */
    private String readClipboard() {
        ClipboardManager manager = (ClipboardManager) getApplicationContext()
                .getSystemService(Context.CLIPBOARD_SERVICE);
        if (manager == null) {
            return "";
        }
        ClipData data = manager.getPrimaryClip();
        if (data == null || data.getItemCount() == 0) {
            return "";
        }
        ClipData.Item item = data.getItemAt(0);
        return item == null ? "" : item.coerceToText(this).toString();
    }

    /** JNI：写系统剪贴板文本。 */
    private void writeClipboard(String value) {
        ClipboardManager manager = (ClipboardManager) getApplicationContext()
                .getSystemService(Context.CLIPBOARD_SERVICE);
        if (manager != null) {
            manager.setPrimaryClip(ClipData.newPlainText("Lumino", value));
        }
    }
}
