package com.flowerguardian.app.tract

import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import java.io.File
import java.util.concurrent.ConcurrentHashMap

/**
 * tract 端侧推理模块。
 *
 * 模型文件放在 `android/app/src/main/assets/models/` 下，首次调用 [loadFromAsset]
 * 时复制到私有目录（tract 需要真实文件路径）。
 *
 * 支持同时驻留多个模型（植物 + 病虫害），以 assetName 作为 key 复用句柄。
 */
class TractEngineModule(private val reactContext: ReactApplicationContext) :
    ReactContextBaseJavaModule(reactContext) {

    private val handles = ConcurrentHashMap<String, Long>()

    init {
        if (libraryLoaded) {
            runCatching { nativeInitLogging() }
        }
    }

    override fun getName(): String = NAME

    @ReactMethod
    fun isAvailable(promise: Promise) {
        promise.resolve(libraryLoaded)
    }

    @ReactMethod
    fun version(promise: Promise) {
        if (!libraryLoaded) {
            promise.resolve(null)
            return
        }
        promise.resolve(nativeVersion())
    }

    /**
     * 从 apk assets 加载模型。
     * @param assetName 形如 `plant_yolo11n.onnx`，相对 assets/models/
     * @param inputSize 导出时的 imgsz，本项目为 640
     * @param numClasses 0 表示由模型输出推断
     */
    @ReactMethod
    fun loadFromAsset(assetName: String, inputSize: Int, numClasses: Int, promise: Promise) {
        try {
            val modelFile = copyAsset("models/$assetName")
            create(assetName, modelFile.absolutePath, inputSize, numClasses)
            promise.resolve(true)
        } catch (e: Throwable) {
            handles.remove(assetName)?.let { if (libraryLoaded) nativeDispose(it) }
            promise.reject("E_TRACT_LOAD", e.message ?: "模型加载失败", e)
        }
    }

    /** 从任意可读路径加载模型（热更新 / 下载到沙盒后使用）。 */
    @ReactMethod
    fun load(modelPath: String, inputSize: Int, numClasses: Int, promise: Promise) {
        try {
            create(modelPath, modelPath, inputSize, numClasses)
            promise.resolve(true)
        } catch (e: Throwable) {
            handles.remove(modelPath)?.let { if (libraryLoaded) nativeDispose(it) }
            promise.reject("E_TRACT_LOAD", e.message ?: "模型加载失败", e)
        }
    }

    /**
     * 对图片文件推理，返回原生 JSON 字符串（JS 侧 JSON.parse）：
     * `{"detections":[{"classId":0,"score":0.9,"x1":..}],"width":..,"height":..,"elapsedMs":..}`
     */
    @ReactMethod
    fun detect(
        modelKey: String,
        imagePath: String,
        confThreshold: Double,
        iouThreshold: Double,
        promise: Promise
    ) {
        val handle = handles[modelKey]
        if (handle == null || handle == 0L) {
            promise.reject("E_TRACT_NOT_LOADED", "模型尚未加载: $modelKey")
            return
        }
        try {
            val json = nativeDetect(
                handle,
                imagePath,
                confThreshold.toFloat(),
                iouThreshold.toFloat()
            )
            if (json == null) {
                promise.reject("E_TRACT_INFER", "推理返回空结果")
            } else {
                promise.resolve(json)
            }
        } catch (e: Throwable) {
            promise.reject("E_TRACT_INFER", e.message ?: "推理失败", e)
        }
    }

    @ReactMethod
    fun dispose(promise: Promise) {
        if (libraryLoaded) {
            handles.values.forEach { nativeDispose(it) }
        }
        handles.clear()
        promise.resolve(true)
    }

    // ---------------------------------------------------------------- internals

    private fun create(key: String, modelPath: String, inputSize: Int, numClasses: Int) {
        if (!libraryLoaded) {
            throw IllegalStateException("libtract_engine.so 未加载，请先执行 build-android.sh")
        }
        handles.remove(key)?.let { nativeDispose(it) }
        val id = nativeCreate(modelPath, inputSize, numClasses)
        if (id <= 0L) {
            throw IllegalStateException("nativeCreate 失败: $modelPath")
        }
        handles[key] = id
    }

    private fun copyAsset(assetPath: String): File {
        val dest = File(File(reactContext.filesDir, "models"), assetPath.substringAfterLast('/'))
        if (dest.exists() && dest.length() > 0L) {
            return dest
        }
        dest.parentFile?.mkdirs()
        reactContext.assets.open(assetPath).use { input ->
            dest.outputStream().use { output -> input.copyTo(output) }
        }
        return dest
    }

    private external fun nativeCreate(modelPath: String, inputSize: Int, numClasses: Int): Long

    private external fun nativeDetect(
        handle: Long,
        imagePath: String,
        confThreshold: Float,
        iouThreshold: Float
    ): String?

    private external fun nativeDispose(handle: Long)

    private external fun nativeVersion(): String?

    private external fun nativeInitLogging()

    companion object {
        const val NAME = "TractEngine"

        private val libraryLoaded: Boolean = try {
            System.loadLibrary("tract_engine")
            true
        } catch (e: Throwable) {
            false
        }
    }
}
