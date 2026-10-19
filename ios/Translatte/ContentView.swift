import PhotosUI
import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var model: TranslatorModel
    @State private var pickerItem: PhotosPickerItem?
    @State private var showCamera = false
    @State private var showOriginal = false
    @State private var selectedRegion: Region?

    var body: some View {
        NavigationStack {
            Group {
                if model.modelsReady {
                    translator
                } else {
                    SetupView()
                }
            }
            .navigationTitle("Translatte")
            .navigationBarTitleDisplayMode(.inline)
        }
    }

    private var translator: some View {
        VStack(spacing: 12) {
            languageBar
            ZStack {
                if let image = model.image {
                    OverlayView(
                        image: image,
                        result: showOriginal ? nil : model.result,
                        rtlTarget: model.targetLanguages.first { $0.code == model.targetCode }?.rtl ?? false,
                        onTap: { selectedRegion = $0 }
                    )
                } else {
                    ContentUnavailableView(
                        "Take or pick a photo",
                        systemImage: "text.viewfinder",
                        description: Text("Text in the photo is translated on your device.")
                    )
                }
                if case let .working(label) = model.phase {
                    ProgressView(label)
                        .padding()
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            if case let .failed(message) = model.phase {
                Text(message).font(.footnote).foregroundStyle(.red)
            }
            if model.result != nil {
                Toggle("Show original", isOn: $showOriginal).padding(.horizontal)
            }
            actions
        }
        .padding(.vertical)
        .sheet(item: $selectedRegion) { region in
            RegionDetail(region: region)
                .presentationDetents([.medium])
        }
        .fullScreenCover(isPresented: $showCamera) {
            CameraPicker { picked in
                showCamera = false
                if let picked { Task { await model.translate(picked) } }
            }
            .ignoresSafeArea()
        }
        .onChange(of: pickerItem) { _, item in
            guard let item else { return }
            Task {
                if let data = try? await item.loadTransferable(type: Data.self), let picked = UIImage(data: data) {
                    await model.translate(picked)
                }
                pickerItem = nil
            }
        }
    }

    private var languageBar: some View {
        HStack {
            Picker("From", selection: $model.sourceCode) {
                ForEach(model.sourceLanguages, id: \.code) { Text($0.name).tag($0.code) }
            }
            Image(systemName: "arrow.right")
            Picker("To", selection: $model.targetCode) {
                ForEach(model.targetLanguages, id: \.code) { Text($0.name).tag($0.code) }
            }
        }
        .pickerStyle(.menu)
    }

    private var actions: some View {
        HStack(spacing: 16) {
            Button {
                showCamera = true
            } label: {
                Label("Camera", systemImage: "camera")
            }
            .disabled(!UIImagePickerController.isSourceTypeAvailable(.camera))
            PhotosPicker(selection: $pickerItem, matching: .images) {
                Label("Photos", systemImage: "photo.on.rectangle")
            }
        }
        .buttonStyle(.borderedProminent)
        .disabled(model.phase.isWorking)
    }
}

private struct RegionDetail: View {
    let region: Region

    var body: some View {
        List {
            Section("Original") { Text(region.sourceText).textSelection(.enabled) }
            Section("Translation") { Text(region.translatedText ?? "Not translated").textSelection(.enabled) }
        }
    }
}

private struct SetupView: View {
    @EnvironmentObject private var model: TranslatorModel

    var body: some View {
        ContentUnavailableView {
            Label("Models not installed", systemImage: "arrow.down.circle")
        } description: {
            VStack(spacing: 8) {
                Text("Translatte needs its OCR and translation models before it can work offline.")
                Text("In-app download is coming. For now, copy a models folder (from `translatte fetch`) into:")
                Text(model.modelsDirectory.path).font(.caption.monospaced()).textSelection(.enabled)
                if let error = model.setupError { Text(error).foregroundStyle(.red) }
            }
        } actions: {
            Button("Check again") { model.reload() }
        }
    }
}

extension Region: Identifiable {
    public var id: String { "\(corners.first?.x ?? 0),\(corners.first?.y ?? 0),\(sourceText)" }
}

extension TranslatorModel.Phase {
    var isWorking: Bool {
        if case .working = self { return true }
        return false
    }
}
