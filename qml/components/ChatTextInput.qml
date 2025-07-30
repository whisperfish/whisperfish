// SPDX-FileCopyrightText: 2021 Mirian Margiani
// SPDX-License-Identifier: AGPL-3.0-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Sailfish.Pickers 1.0
import Nemo.Time 1.0
import be.rubdos.whisperfish 1.0
import "../pages"

Item {
    id: root
    width: parent.width
    height: column.height + Theme.paddingSmall

    property alias text: input.text
    // contents: [{data: path, type: mimetype}, ...]
    property var attachments: ([]) // always update via assignment to ensure notifications
    property alias textPlaceholder: input.placeholderText
    property alias editor: input

    // A personalized placeholder should only be shown when starting a new 1:1 chat.
    property bool enablePersonalizedPlaceholder: false
    property string placeholderContactName: ''
    property int maxHeight: 3*Theme.itemSizeLarge // TODO adapt based on screen size
    property bool showSeparator: false
    property bool clearAfterSend: true
    property bool enableSending: true
    property bool enableAttachments: true
    property bool dockMoving
    property bool enableTypingIndicators: SettingsBridge.enable_typing_indicators
    property bool isGroup

    property bool isVoiceNote: false
    property var voiceNoteStartTime: null
    // In seconds
    property var voiceNoteDuration: 0;

    // getTime() doesn't work in a declarative context, so we need a timer
    Timer {
        running: voiceNoteStartTime != null
        repeat: true
        interval: 100
        onTriggered: {
            voiceNoteDuration = (new Date().getTime() - voiceNoteStartTime) / 1000;
        }
    }

    readonly property bool quotedMessageShown: quoteItem.messageId >= 0
    readonly property bool canSend: enableSending &&
                                    ClientWorker.connected &&
                                    (text.trim().length > 0 ||
                                     attachments.length > 0 ||
                                     recorder.isRecording)

    signal sendMessage(var text, var attachments, var replyTo /* message id */, var isVoiceNote)
    signal sendTypingNotification()
    signal sendTypingNotificationEnd()
    signal quotedMessageClicked(var messageId)

    onDockMovingChanged: {
        if(buttonContainer.enabled) {
            inputRow.toggleAttachmentButtons()
        }
    }

    function reset() {
        Qt.inputMethod.commit()
        text = ""
        attachments = []
        resetQuote()
        isVoiceNote = false
        voiceNoteStartTime = null;

        if (input.focus) { // reset keyboard state
            input.focus = false
            input.focus = true
        }
    }

    function setQuote(index, modelData) {
        quoteItem.messageId = modelData.id
    }

    function resetQuote() {
        quoteItem.messageId = -1
    }

    function forceEditorFocus(/*bool*/ atEnd) {
        if (atEnd) input.cursorPosition = input.text.length
        input.forceActiveFocus()
    }

    function _send() {
        Qt.inputMethod.commit()
        if (isVoiceNote) {
            var filename = recorder.stop();
            var type;
            if (useAac()) {
                type = "audio/aac";
            } else {
                type = "audio/ogg";
            }
            attachments = [{data: filename, type: type}];
        }
        if (text.length === 0 && attachments.length === 0) return
        if(SettingsBridge.enable_enter_send) {
            text = text.replace(/(\r\n\t|\n|\r\t)/gm, '')
        }
        sendMessage(text, attachments, quoteItem.messageId, isVoiceNote)
        if (clearAfterSend) reset()
    }

    function useAac() {
        // TODO: Vorbis is not supported at all on iOS, so we need to use AAC there.
        //       https://github.com/signalapp/Signal-iOS/issues/4539
        //       https://github.com/signalapp/Signal-iOS/issues/5771
        // TODO: Jolla's gstreamer version is 1.14.5, which crashes on libav_aacenc, so on gstreamer lower than 1.22,
        //       we use Vorbis.  1.22 is tested on Sailfish 4.6.
        //       This means that voice messages sent from SailfishOS 3.4 will not be playable on iOS.
        //       Sad panda. 🐼
        return AppState.gstreamer_version_major > 1
            || AppState.gstreamer_version_major == 1 && AppState.gstreamer_version_minor >= 22;
    }

    function startRecording() {
        isVoiceNote = true;
        var ext;
        if (useAac()) {
            ext = "aac";
        } else {
            ext = "ogg";
        }
        var path = SettingsBridge.voice_note_dir + "/Note_" + Qt.formatDateTime(new Date(), "yyyyMMdd_hhmmss") + "." + ext
        recorder.start(path);
        voiceNoteStartTime = new Date().getTime();
    }

    function cancelRecording() {
        isVoiceNote = false;
        recorder.stop();
        recorder.reset();
        voiceNoteStartTime = null;
    }

    VoiceNoteRecorder {
        id: recorder
    }

    WallClock {
        id: clock
        enabled: parent.enabled && Qt.application.active
        updateFrequency: WallClock.Minute
    }

    Separator {
        opacity: showSeparator ? Theme.opacityHigh : 0.0
        color: input.focus ? Theme.secondaryHighlightColor :
                             Theme.secondaryColor
        horizontalAlignment: Qt.AlignHCenter
        anchors {
            left: parent.left; leftMargin: Theme.horizontalPageMargin
            right: parent.right; rightMargin: Theme.horizontalPageMargin
            top: parent.top
        }
        Behavior on opacity { FadeAnimator { } }
    }

    Timer {
        id: isTypingTimer
        running: false
        repeat: false
        // XXX Fine tune the timer values -- this should be longer
        interval: 5000
        property bool shouldSend: false
        onShouldSendChanged: {
            if(enableTypingIndicators && shouldSend) {
                if(!running) {
                    sendTypingNotification()
                    shouldSend = false
                    start()
                }
                shouldSend = false
            }
        }
        onTriggered: {
            if(shouldSend) {
                restart()
            } else {
                sendTypingNotificationEnd()
            }
        }
        Component.onDestruction: stop()
    }

    Timer {
        id: isNotTypingTimer
        running: false
        repeat: false
        interval: 6000
        onTriggered: sendTypingNotificationEnd()
        Component.onDestruction: {
            if(running) {
                stop()
                sendTypingNotificationEnd()
            }
        }
    }

    Column {
        id: column
        width: parent.width
        height: input.height + spacing + quoteItem.height
        anchors.bottom: parent.bottom
        spacing: Theme.paddingSmall

        QuotedMessagePreview {
            id: quoteItem
            width: parent.width - 2*Theme.horizontalPageMargin
            anchors.horizontalCenter: parent.horizontalCenter
            showCloseButton: true
            showBackground: false
            messageId: -1 // set through setQuote()/resetQuote()
            clip: true // for slide animation
            Behavior on height { SmoothedAnimation { duration: 120 } }
            onClicked: quotedMessageClicked(quoteItem.messageId)
            onCloseClicked: resetQuote()
        }

        Item {
            id: inputRow
            anchors { left: parent.left; right: parent.right }
            height: input.height

            function toggleAttachmentButtons() {
                if(buttonContainer.enabled) {
                    buttonContainer.enabled = false
                    buttonContainer.opacity = 0.0
                    moreButton.iconRotation = 0
                }
                else {
                    buttonContainer.enabled = true
                    buttonContainer.opacity = 1.0
                    moreButton.iconRotation = 45
                }
            }

            Image {
                id: voiceNoteRecordingIcon
                source: "../../icons/microphone.png"
                width: height
                height: parent.height - 2* Theme.paddingMedium

                visible: isVoiceNote

                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    bottom: parent.bottom
                    bottomMargin: Theme.paddingMedium
                }
            }

            Label {
                id: voiceNoteRecordingTime
                anchors {
                    left: voiceNoteRecordingIcon.right
                    leftMargin: Theme.paddingMedium
                    verticalCenter: parent.verticalCenter
                }
                visible: isVoiceNote
                height: parent.height
                font.pixelSize: if (useAac()) {
                    Theme.fontSizeMedium
                } else {
                    Theme.fontSizeTiny
                }

                function formatTime(dt) {
                    var minutes, seconds;
                    minutes = Math.floor(dt / 60);
                    seconds = Math.floor(dt % 60);
                    var s = minutes + ":" + (seconds < 10 ? "0" : "") + seconds;
                    if (useAac()) {
                        return s;
                    } else {
                        //: Short warning note that the voice note is being recorded in Vorbis format
                        //% "Incompatible with Signal iOS"
                        return qsTrId("whisperfish-voice-note-vorbis-warning") + " " + s;
                    }
                }

                text: formatTime(voiceNoteDuration)
                verticalAlignment: Text.AlignVCenter
            }

            TextArea {
                id: input

                visible: !isVoiceNote

                property real minInputHeight: Theme.itemSizeMedium
                property real maxInputHeight: maxHeight - column.spacing - quoteItem.height
                height: implicitHeight < maxInputHeight ?
                            (implicitHeight > minInputHeight ? implicitHeight : minInputHeight) :
                            maxInputHeight
                width: parent.width - attachButton.width - sendButton.width -
                       2*Theme.paddingSmall - Theme.horizontalPageMargin
                anchors {
                    bottom: parent.bottom; bottomMargin: -Theme.paddingSmall
                    left: parent.left
                    right: moreButton.left; rightMargin: Theme.paddingSmall
                }
                label: Format.formatDate(clock.time, Formatter.TimeValue) +
                       (attachments.length > 0 ?
                            " — " +
                            //: Number of attachments currently selected for sending
                            //% "%n attachment(s)"
                            qsTrId("whisperfish-chat-input-attachment-label", attachments.length) :
                            "")
                hideLabelOnEmptyField: false
                textRightMargin: 0
                font.pixelSize: Theme.fontSizeSmall
                enabled: (enableSending || text.length > 0) && !isVoiceNote
                placeholderText: if (!enableSending) {
                        if (isGroup) {
                            //: Chat text input placeholder for not being a member of the group
                            //% "You are not member of the group"
                            qsTrId("whisperfish-chat-input-not-group-member")
                        } else {
                            //: Chat text input placeholder for deleted/unregistered recipient
                            //% "The recipient is not registered"
                            qsTrId("whisperfish-chat-input-recipient-is-unregistered")
                        }
                    } else if ((enablePersonalizedPlaceholder || isLandscape) && placeholderContactName.length > 0) {
                        //: Personalized placeholder for chat input, e.g. "Hi John"
                        //% "Hi %1"
                        qsTrId("whisperfish-chat-input-placeholder-personal").arg(placeholderContactName)
                    } else {
                        //: Generic placeholder for chat input
                        //% "Write a message"
                        qsTrId("whisperfish-chat-input-placeholder-default")
                    }

                focusOutBehavior: FocusBehavior.KeepFocus

                EnterKey.onClicked: {
                    if (canSend && SettingsBridge.enable_enter_send) {
                        _send()
                    }
                }

                onTextChanged: {
                    if(enableTypingIndicators) {
                        isTypingTimer.shouldSend = text.length > 0;
                        isNotTypingTimer.restart()
                    }
                }
            }

            IconButton {
                id: moreButton
                enabled: enableSending && !isVoiceNote
                visible: enableAttachments && !isVoiceNote
                anchors {
                    right: sendButton.left; rightMargin: Theme.paddingSmall
                    bottom: parent.bottom; bottomMargin: Theme.paddingMedium
                }
                icon.source: "image://theme/icon-m-add"
                icon.width: enableAttachments ? Theme.iconSizeMedium : 0
                icon.height: icon.width
                icon.rotation: iconRotation
                property real iconRotation: 0
                Behavior on iconRotation {
                    NumberAnimation {
                        duration: 200
                    }
                }
                onClicked: inputRow.toggleAttachmentButtons()
            }

            Item {
                id: buttonContainer
                anchors {
                    horizontalCenter: moreButton.horizontalCenter
                    bottom: moreButton.top
                }
                width: cameraButton.width
                height: voiceButton.height + cameraButton.height + attachButton.height + (3 * Theme.paddingSmall)

                clip: false

                enabled: false
                opacity: 0.0
                visible: opacity > 0.0

                Behavior on opacity {
                    NumberAnimation {
                        duration: 200
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    radius: width / 4.0
                    color: Theme.rgba(Theme.highlightDimmerColor, 0.9)
                }

                IconButton {
                    id: voiceButton
                    anchors {
                        top: parent.top
                        horizontalCenter: parent.horizontalCenter
                    }
                    icon.source: "../../icons/microphone.png"
                    icon.width: enableAttachments ? Theme.iconSizeMedium : 0
                    icon.height: icon.width
                    visible: enableAttachments
                    onClicked: {
                        inputRow.toggleAttachmentButtons();
                        startRecording();
                    }
                }

                IconButton {
                    id: cameraButton
                    anchors {
                        top: voiceButton.bottom
                        topMargin: Theme.paddingSmall
                        horizontalCenter: parent.horizontalCenter
                    }
                    icon.source: "image://theme/icon-m-camera"
                    icon.width: enableAttachments ? Theme.iconSizeMedium : 0
                    icon.height: icon.width
                    visible: enableAttachments
                    onClicked: {
                        inputRow.toggleAttachmentButtons()
                        pageStack.push(cameraDialog)
                    }
                }

                IconButton {
                    id: attachButton
                    anchors {
                        top: cameraButton.bottom
                        topMargin: Theme.paddingSmall
                        horizontalCenter: parent.horizontalCenter
                    }
                    icon.source: "image://theme/icon-m-attach"
                    icon.width: enableAttachments ? Theme.iconSizeMedium : 0
                    icon.height: icon.width
                    visible: enableAttachments
                    onClicked: {
                        inputRow.toggleAttachmentButtons()
                        pageStack.push(multiDocumentPickerDialog)
                    }
                }
            }

            IconButton {
                id: cancelButton
                anchors {
                    // icon-m-cancel has own padding
                    right: sendButton.left; rightMargin: Theme.paddingMedium
                    bottom: parent.bottom; bottomMargin: Theme.paddingMedium
                }
                icon.width: Theme.iconSizeMedium + 2*Theme.paddingSmall
                icon.height: width
                icon.source: "image://theme/icon-m-cancel"
                visible: isVoiceNote
                enabled: isVoiceNote
                onClicked: {
                    cancelRecording()
                }
            }

            IconButton {
                id: sendButton
                anchors {
                    // icon-m-send has own padding
                    right: parent.right; rightMargin: Theme.horizontalPageMargin-Theme.paddingMedium
                    bottom: parent.bottom; bottomMargin: Theme.paddingMedium
                }
                icon.width: Theme.iconSizeMedium + 2*Theme.paddingSmall
                icon.height: width
                icon.source: ClientWorker.connected ? "image://theme/icon-m-send" : "image://theme/icon-s-blocked"
                enabled: canSend
                onClicked: {
                    if (canSend /*&& SettingsBridge.send_on_click*/) {
                        _send()
                        isTypingTimer.stop()
                        isNotTypingTimer.stop()
                    }
                    if (buttonContainer.enabled) {
                        inputRow.toggleAttachmentButtons()
                    }
                }
                onPressAndHold: {
                    // TODO implement in backend
                    if (canSend /*&& SettingsBridge.send_on_click === false*/) {
                        _send()
                        isTypingTimer.stop()
                        isNotTypingTimer.stop()
                    }
                }
            }

            Component {
                id: cameraDialog
                CameraDialog {
                    onAccepted: {
                        var newAttachments = []
                            newAttachments.push({data: fileName, type: fileType})
                        root.attachments = newAttachments // assignment to update bindings
                    }
                    onRejected: {
                        // Rejecting the dialog should not unexpectedly clear the
                        // currently selected attachments.
                        // root.attachments = []
                    }
                }
            }

            Component {
                id: multiDocumentPickerDialog
                MultiContentPickerDialog {
                    //: Attachment picker page title
                    //% "Select attachments"
                    title: qsTrId("whisperfish-select-attachments-page-title")
                    onAccepted: {
                        var newAttachments = []
                        for (var i = 0; i < selectedContent.count; i++) {
                            var item = selectedContent.get(i)
                            newAttachments.push({data: item.filePath, type: item.mimeType})
                        }
                        root.attachments = newAttachments // assignment to update bindings
                    }
                    onRejected: {
                        // Rejecting the dialog should not unexpectedly clear the
                        // currently selected attachments.
                        // root.attachments = []
                    }
                }
            }
        }
    }
}
