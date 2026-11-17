// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'types.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$ChatBody {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ChatBody);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'ChatBody()';
}


}

/// @nodoc
class $ChatBodyCopyWith<$Res>  {
$ChatBodyCopyWith(ChatBody _, $Res Function(ChatBody) __);
}


/// Adds pattern-matching-related methods to [ChatBody].
extension ChatBodyPatterns on ChatBody {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( ChatBody_Text value)?  text,TResult Function( ChatBody_Unsupported value)?  unsupported,required TResult orElse(),}){
final _that = this;
switch (_that) {
case ChatBody_Text() when text != null:
return text(_that);case ChatBody_Unsupported() when unsupported != null:
return unsupported(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( ChatBody_Text value)  text,required TResult Function( ChatBody_Unsupported value)  unsupported,}){
final _that = this;
switch (_that) {
case ChatBody_Text():
return text(_that);case ChatBody_Unsupported():
return unsupported(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( ChatBody_Text value)?  text,TResult? Function( ChatBody_Unsupported value)?  unsupported,}){
final _that = this;
switch (_that) {
case ChatBody_Text() when text != null:
return text(_that);case ChatBody_Unsupported() when unsupported != null:
return unsupported(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String text)?  text,TResult Function()?  unsupported,required TResult orElse(),}) {final _that = this;
switch (_that) {
case ChatBody_Text() when text != null:
return text(_that.text);case ChatBody_Unsupported() when unsupported != null:
return unsupported();case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String text)  text,required TResult Function()  unsupported,}) {final _that = this;
switch (_that) {
case ChatBody_Text():
return text(_that.text);case ChatBody_Unsupported():
return unsupported();}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String text)?  text,TResult? Function()?  unsupported,}) {final _that = this;
switch (_that) {
case ChatBody_Text() when text != null:
return text(_that.text);case ChatBody_Unsupported() when unsupported != null:
return unsupported();case _:
  return null;

}
}

}

/// @nodoc


class ChatBody_Text extends ChatBody {
  const ChatBody_Text({required this.text}): super._();
  

/// The text, only to be shown.
 final  String text;

/// Create a copy of ChatBody
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ChatBody_TextCopyWith<ChatBody_Text> get copyWith => _$ChatBody_TextCopyWithImpl<ChatBody_Text>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ChatBody_Text&&(identical(other.text, text) || other.text == text));
}


@override
int get hashCode => Object.hash(runtimeType,text);

@override
String toString() {
  return 'ChatBody.text(text: $text)';
}


}

/// @nodoc
abstract mixin class $ChatBody_TextCopyWith<$Res> implements $ChatBodyCopyWith<$Res> {
  factory $ChatBody_TextCopyWith(ChatBody_Text value, $Res Function(ChatBody_Text) _then) = _$ChatBody_TextCopyWithImpl;
@useResult
$Res call({
 String text
});




}
/// @nodoc
class _$ChatBody_TextCopyWithImpl<$Res>
    implements $ChatBody_TextCopyWith<$Res> {
  _$ChatBody_TextCopyWithImpl(this._self, this._then);

  final ChatBody_Text _self;
  final $Res Function(ChatBody_Text) _then;

/// Create a copy of ChatBody
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? text = null,}) {
  return _then(ChatBody_Text(
text: null == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class ChatBody_Unsupported extends ChatBody {
  const ChatBody_Unsupported(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ChatBody_Unsupported);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'ChatBody.unsupported()';
}


}




/// @nodoc
mixin _$CoreEvent {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreEvent()';
}


}

/// @nodoc
class $CoreEventCopyWith<$Res>  {
$CoreEventCopyWith(CoreEvent _, $Res Function(CoreEvent) __);
}


/// Adds pattern-matching-related methods to [CoreEvent].
extension CoreEventPatterns on CoreEvent {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( CoreEvent_Message value)?  message,TResult Function( CoreEvent_SendStatus value)?  sendStatus,TResult Function( CoreEvent_ProfileChanged value)?  profileChanged,TResult Function( CoreEvent_GroupSettingsChanged value)?  groupSettingsChanged,TResult Function( CoreEvent_Unreadable value)?  unreadable,TResult Function( CoreEvent_Gap value)?  gap,TResult Function( CoreEvent_RemovedForInactivity value)?  removedForInactivity,TResult Function( CoreEvent_Rejoined value)?  rejoined,TResult Function( CoreEvent_IdentityKeyChanged value)?  identityKeyChanged,TResult Function( CoreEvent_ServerIdentityError value)?  serverIdentityError,TResult Function( CoreEvent_Removed value)?  removed,TResult Function( CoreEvent_JoinedGroup value)?  joinedGroup,TResult Function( CoreEvent_MembersChanged value)?  membersChanged,TResult Function( CoreEvent_CommitRefused value)?  commitRefused,TResult Function( CoreEvent_ReaddRequested value)?  readdRequested,TResult Function( CoreEvent_Connection value)?  connection,TResult Function( CoreEvent_Other value)?  other,required TResult orElse(),}){
final _that = this;
switch (_that) {
case CoreEvent_Message() when message != null:
return message(_that);case CoreEvent_SendStatus() when sendStatus != null:
return sendStatus(_that);case CoreEvent_ProfileChanged() when profileChanged != null:
return profileChanged(_that);case CoreEvent_GroupSettingsChanged() when groupSettingsChanged != null:
return groupSettingsChanged(_that);case CoreEvent_Unreadable() when unreadable != null:
return unreadable(_that);case CoreEvent_Gap() when gap != null:
return gap(_that);case CoreEvent_RemovedForInactivity() when removedForInactivity != null:
return removedForInactivity(_that);case CoreEvent_Rejoined() when rejoined != null:
return rejoined(_that);case CoreEvent_IdentityKeyChanged() when identityKeyChanged != null:
return identityKeyChanged(_that);case CoreEvent_ServerIdentityError() when serverIdentityError != null:
return serverIdentityError(_that);case CoreEvent_Removed() when removed != null:
return removed(_that);case CoreEvent_JoinedGroup() when joinedGroup != null:
return joinedGroup(_that);case CoreEvent_MembersChanged() when membersChanged != null:
return membersChanged(_that);case CoreEvent_CommitRefused() when commitRefused != null:
return commitRefused(_that);case CoreEvent_ReaddRequested() when readdRequested != null:
return readdRequested(_that);case CoreEvent_Connection() when connection != null:
return connection(_that);case CoreEvent_Other() when other != null:
return other(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( CoreEvent_Message value)  message,required TResult Function( CoreEvent_SendStatus value)  sendStatus,required TResult Function( CoreEvent_ProfileChanged value)  profileChanged,required TResult Function( CoreEvent_GroupSettingsChanged value)  groupSettingsChanged,required TResult Function( CoreEvent_Unreadable value)  unreadable,required TResult Function( CoreEvent_Gap value)  gap,required TResult Function( CoreEvent_RemovedForInactivity value)  removedForInactivity,required TResult Function( CoreEvent_Rejoined value)  rejoined,required TResult Function( CoreEvent_IdentityKeyChanged value)  identityKeyChanged,required TResult Function( CoreEvent_ServerIdentityError value)  serverIdentityError,required TResult Function( CoreEvent_Removed value)  removed,required TResult Function( CoreEvent_JoinedGroup value)  joinedGroup,required TResult Function( CoreEvent_MembersChanged value)  membersChanged,required TResult Function( CoreEvent_CommitRefused value)  commitRefused,required TResult Function( CoreEvent_ReaddRequested value)  readdRequested,required TResult Function( CoreEvent_Connection value)  connection,required TResult Function( CoreEvent_Other value)  other,}){
final _that = this;
switch (_that) {
case CoreEvent_Message():
return message(_that);case CoreEvent_SendStatus():
return sendStatus(_that);case CoreEvent_ProfileChanged():
return profileChanged(_that);case CoreEvent_GroupSettingsChanged():
return groupSettingsChanged(_that);case CoreEvent_Unreadable():
return unreadable(_that);case CoreEvent_Gap():
return gap(_that);case CoreEvent_RemovedForInactivity():
return removedForInactivity(_that);case CoreEvent_Rejoined():
return rejoined(_that);case CoreEvent_IdentityKeyChanged():
return identityKeyChanged(_that);case CoreEvent_ServerIdentityError():
return serverIdentityError(_that);case CoreEvent_Removed():
return removed(_that);case CoreEvent_JoinedGroup():
return joinedGroup(_that);case CoreEvent_MembersChanged():
return membersChanged(_that);case CoreEvent_CommitRefused():
return commitRefused(_that);case CoreEvent_ReaddRequested():
return readdRequested(_that);case CoreEvent_Connection():
return connection(_that);case CoreEvent_Other():
return other(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( CoreEvent_Message value)?  message,TResult? Function( CoreEvent_SendStatus value)?  sendStatus,TResult? Function( CoreEvent_ProfileChanged value)?  profileChanged,TResult? Function( CoreEvent_GroupSettingsChanged value)?  groupSettingsChanged,TResult? Function( CoreEvent_Unreadable value)?  unreadable,TResult? Function( CoreEvent_Gap value)?  gap,TResult? Function( CoreEvent_RemovedForInactivity value)?  removedForInactivity,TResult? Function( CoreEvent_Rejoined value)?  rejoined,TResult? Function( CoreEvent_IdentityKeyChanged value)?  identityKeyChanged,TResult? Function( CoreEvent_ServerIdentityError value)?  serverIdentityError,TResult? Function( CoreEvent_Removed value)?  removed,TResult? Function( CoreEvent_JoinedGroup value)?  joinedGroup,TResult? Function( CoreEvent_MembersChanged value)?  membersChanged,TResult? Function( CoreEvent_CommitRefused value)?  commitRefused,TResult? Function( CoreEvent_ReaddRequested value)?  readdRequested,TResult? Function( CoreEvent_Connection value)?  connection,TResult? Function( CoreEvent_Other value)?  other,}){
final _that = this;
switch (_that) {
case CoreEvent_Message() when message != null:
return message(_that);case CoreEvent_SendStatus() when sendStatus != null:
return sendStatus(_that);case CoreEvent_ProfileChanged() when profileChanged != null:
return profileChanged(_that);case CoreEvent_GroupSettingsChanged() when groupSettingsChanged != null:
return groupSettingsChanged(_that);case CoreEvent_Unreadable() when unreadable != null:
return unreadable(_that);case CoreEvent_Gap() when gap != null:
return gap(_that);case CoreEvent_RemovedForInactivity() when removedForInactivity != null:
return removedForInactivity(_that);case CoreEvent_Rejoined() when rejoined != null:
return rejoined(_that);case CoreEvent_IdentityKeyChanged() when identityKeyChanged != null:
return identityKeyChanged(_that);case CoreEvent_ServerIdentityError() when serverIdentityError != null:
return serverIdentityError(_that);case CoreEvent_Removed() when removed != null:
return removed(_that);case CoreEvent_JoinedGroup() when joinedGroup != null:
return joinedGroup(_that);case CoreEvent_MembersChanged() when membersChanged != null:
return membersChanged(_that);case CoreEvent_CommitRefused() when commitRefused != null:
return commitRefused(_that);case CoreEvent_ReaddRequested() when readdRequested != null:
return readdRequested(_that);case CoreEvent_Connection() when connection != null:
return connection(_that);case CoreEvent_Other() when other != null:
return other(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( ChatMessage message)?  message,TResult Function( PlatformInt64 id,  SendState status)?  sendStatus,TResult Function( UserRef user,  String? displayName,  String? about)?  profileChanged,TResult Function( String? name,  String? previous,  UserRef by)?  groupSettingsChanged,TResult Function( UserRef? sender,  PlatformInt64? acceptedAtMs)?  unreadable,TResult Function( PlatformInt64 newestExpiredAtMs)?  gap,TResult Function( PlatformInt64 missedSinceMs)?  removedForInactivity,TResult Function()?  rejoined,TResult Function( UserRef user)?  identityKeyChanged,TResult Function()?  serverIdentityError,TResult Function()?  removed,TResult Function()?  joinedGroup,TResult Function( List<MemberDevice> added,  List<MemberDevice> removed,  UserRef? by,  List<MemberDevice> automatic)?  membersChanged,TResult Function( String reason)?  commitRefused,TResult Function( ReaddAnswer outcome)?  readdRequested,TResult Function( ConnectionStatus status)?  connection,TResult Function()?  other,required TResult orElse(),}) {final _that = this;
switch (_that) {
case CoreEvent_Message() when message != null:
return message(_that.message);case CoreEvent_SendStatus() when sendStatus != null:
return sendStatus(_that.id,_that.status);case CoreEvent_ProfileChanged() when profileChanged != null:
return profileChanged(_that.user,_that.displayName,_that.about);case CoreEvent_GroupSettingsChanged() when groupSettingsChanged != null:
return groupSettingsChanged(_that.name,_that.previous,_that.by);case CoreEvent_Unreadable() when unreadable != null:
return unreadable(_that.sender,_that.acceptedAtMs);case CoreEvent_Gap() when gap != null:
return gap(_that.newestExpiredAtMs);case CoreEvent_RemovedForInactivity() when removedForInactivity != null:
return removedForInactivity(_that.missedSinceMs);case CoreEvent_Rejoined() when rejoined != null:
return rejoined();case CoreEvent_IdentityKeyChanged() when identityKeyChanged != null:
return identityKeyChanged(_that.user);case CoreEvent_ServerIdentityError() when serverIdentityError != null:
return serverIdentityError();case CoreEvent_Removed() when removed != null:
return removed();case CoreEvent_JoinedGroup() when joinedGroup != null:
return joinedGroup();case CoreEvent_MembersChanged() when membersChanged != null:
return membersChanged(_that.added,_that.removed,_that.by,_that.automatic);case CoreEvent_CommitRefused() when commitRefused != null:
return commitRefused(_that.reason);case CoreEvent_ReaddRequested() when readdRequested != null:
return readdRequested(_that.outcome);case CoreEvent_Connection() when connection != null:
return connection(_that.status);case CoreEvent_Other() when other != null:
return other();case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( ChatMessage message)  message,required TResult Function( PlatformInt64 id,  SendState status)  sendStatus,required TResult Function( UserRef user,  String? displayName,  String? about)  profileChanged,required TResult Function( String? name,  String? previous,  UserRef by)  groupSettingsChanged,required TResult Function( UserRef? sender,  PlatformInt64? acceptedAtMs)  unreadable,required TResult Function( PlatformInt64 newestExpiredAtMs)  gap,required TResult Function( PlatformInt64 missedSinceMs)  removedForInactivity,required TResult Function()  rejoined,required TResult Function( UserRef user)  identityKeyChanged,required TResult Function()  serverIdentityError,required TResult Function()  removed,required TResult Function()  joinedGroup,required TResult Function( List<MemberDevice> added,  List<MemberDevice> removed,  UserRef? by,  List<MemberDevice> automatic)  membersChanged,required TResult Function( String reason)  commitRefused,required TResult Function( ReaddAnswer outcome)  readdRequested,required TResult Function( ConnectionStatus status)  connection,required TResult Function()  other,}) {final _that = this;
switch (_that) {
case CoreEvent_Message():
return message(_that.message);case CoreEvent_SendStatus():
return sendStatus(_that.id,_that.status);case CoreEvent_ProfileChanged():
return profileChanged(_that.user,_that.displayName,_that.about);case CoreEvent_GroupSettingsChanged():
return groupSettingsChanged(_that.name,_that.previous,_that.by);case CoreEvent_Unreadable():
return unreadable(_that.sender,_that.acceptedAtMs);case CoreEvent_Gap():
return gap(_that.newestExpiredAtMs);case CoreEvent_RemovedForInactivity():
return removedForInactivity(_that.missedSinceMs);case CoreEvent_Rejoined():
return rejoined();case CoreEvent_IdentityKeyChanged():
return identityKeyChanged(_that.user);case CoreEvent_ServerIdentityError():
return serverIdentityError();case CoreEvent_Removed():
return removed();case CoreEvent_JoinedGroup():
return joinedGroup();case CoreEvent_MembersChanged():
return membersChanged(_that.added,_that.removed,_that.by,_that.automatic);case CoreEvent_CommitRefused():
return commitRefused(_that.reason);case CoreEvent_ReaddRequested():
return readdRequested(_that.outcome);case CoreEvent_Connection():
return connection(_that.status);case CoreEvent_Other():
return other();}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( ChatMessage message)?  message,TResult? Function( PlatformInt64 id,  SendState status)?  sendStatus,TResult? Function( UserRef user,  String? displayName,  String? about)?  profileChanged,TResult? Function( String? name,  String? previous,  UserRef by)?  groupSettingsChanged,TResult? Function( UserRef? sender,  PlatformInt64? acceptedAtMs)?  unreadable,TResult? Function( PlatformInt64 newestExpiredAtMs)?  gap,TResult? Function( PlatformInt64 missedSinceMs)?  removedForInactivity,TResult? Function()?  rejoined,TResult? Function( UserRef user)?  identityKeyChanged,TResult? Function()?  serverIdentityError,TResult? Function()?  removed,TResult? Function()?  joinedGroup,TResult? Function( List<MemberDevice> added,  List<MemberDevice> removed,  UserRef? by,  List<MemberDevice> automatic)?  membersChanged,TResult? Function( String reason)?  commitRefused,TResult? Function( ReaddAnswer outcome)?  readdRequested,TResult? Function( ConnectionStatus status)?  connection,TResult? Function()?  other,}) {final _that = this;
switch (_that) {
case CoreEvent_Message() when message != null:
return message(_that.message);case CoreEvent_SendStatus() when sendStatus != null:
return sendStatus(_that.id,_that.status);case CoreEvent_ProfileChanged() when profileChanged != null:
return profileChanged(_that.user,_that.displayName,_that.about);case CoreEvent_GroupSettingsChanged() when groupSettingsChanged != null:
return groupSettingsChanged(_that.name,_that.previous,_that.by);case CoreEvent_Unreadable() when unreadable != null:
return unreadable(_that.sender,_that.acceptedAtMs);case CoreEvent_Gap() when gap != null:
return gap(_that.newestExpiredAtMs);case CoreEvent_RemovedForInactivity() when removedForInactivity != null:
return removedForInactivity(_that.missedSinceMs);case CoreEvent_Rejoined() when rejoined != null:
return rejoined();case CoreEvent_IdentityKeyChanged() when identityKeyChanged != null:
return identityKeyChanged(_that.user);case CoreEvent_ServerIdentityError() when serverIdentityError != null:
return serverIdentityError();case CoreEvent_Removed() when removed != null:
return removed();case CoreEvent_JoinedGroup() when joinedGroup != null:
return joinedGroup();case CoreEvent_MembersChanged() when membersChanged != null:
return membersChanged(_that.added,_that.removed,_that.by,_that.automatic);case CoreEvent_CommitRefused() when commitRefused != null:
return commitRefused(_that.reason);case CoreEvent_ReaddRequested() when readdRequested != null:
return readdRequested(_that.outcome);case CoreEvent_Connection() when connection != null:
return connection(_that.status);case CoreEvent_Other() when other != null:
return other();case _:
  return null;

}
}

}

/// @nodoc


class CoreEvent_Message extends CoreEvent {
  const CoreEvent_Message({required this.message}): super._();
  

/// The message.
 final  ChatMessage message;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_MessageCopyWith<CoreEvent_Message> get copyWith => _$CoreEvent_MessageCopyWithImpl<CoreEvent_Message>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_Message&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,message);

@override
String toString() {
  return 'CoreEvent.message(message: $message)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_MessageCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_MessageCopyWith(CoreEvent_Message value, $Res Function(CoreEvent_Message) _then) = _$CoreEvent_MessageCopyWithImpl;
@useResult
$Res call({
 ChatMessage message
});




}
/// @nodoc
class _$CoreEvent_MessageCopyWithImpl<$Res>
    implements $CoreEvent_MessageCopyWith<$Res> {
  _$CoreEvent_MessageCopyWithImpl(this._self, this._then);

  final CoreEvent_Message _self;
  final $Res Function(CoreEvent_Message) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(CoreEvent_Message(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as ChatMessage,
  ));
}


}

/// @nodoc


class CoreEvent_SendStatus extends CoreEvent {
  const CoreEvent_SendStatus({required this.id, required this.status}): super._();
  

/// The message's local id.
 final  PlatformInt64 id;
/// Its new status.
 final  SendState status;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_SendStatusCopyWith<CoreEvent_SendStatus> get copyWith => _$CoreEvent_SendStatusCopyWithImpl<CoreEvent_SendStatus>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_SendStatus&&(identical(other.id, id) || other.id == id)&&(identical(other.status, status) || other.status == status));
}


@override
int get hashCode => Object.hash(runtimeType,id,status);

@override
String toString() {
  return 'CoreEvent.sendStatus(id: $id, status: $status)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_SendStatusCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_SendStatusCopyWith(CoreEvent_SendStatus value, $Res Function(CoreEvent_SendStatus) _then) = _$CoreEvent_SendStatusCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 id, SendState status
});


$SendStateCopyWith<$Res> get status;

}
/// @nodoc
class _$CoreEvent_SendStatusCopyWithImpl<$Res>
    implements $CoreEvent_SendStatusCopyWith<$Res> {
  _$CoreEvent_SendStatusCopyWithImpl(this._self, this._then);

  final CoreEvent_SendStatus _self;
  final $Res Function(CoreEvent_SendStatus) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? id = null,Object? status = null,}) {
  return _then(CoreEvent_SendStatus(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as PlatformInt64,status: null == status ? _self.status : status // ignore: cast_nullable_to_non_nullable
as SendState,
  ));
}

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$SendStateCopyWith<$Res> get status {
  
  return $SendStateCopyWith<$Res>(_self.status, (value) {
    return _then(_self.copyWith(status: value));
  });
}
}

/// @nodoc


class CoreEvent_ProfileChanged extends CoreEvent {
  const CoreEvent_ProfileChanged({required this.user, this.displayName, this.about}): super._();
  

/// The user.
 final  UserRef user;
/// The display name, if set.
 final  String? displayName;
/// The "about" text, if set.
 final  String? about;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_ProfileChangedCopyWith<CoreEvent_ProfileChanged> get copyWith => _$CoreEvent_ProfileChangedCopyWithImpl<CoreEvent_ProfileChanged>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_ProfileChanged&&(identical(other.user, user) || other.user == user)&&(identical(other.displayName, displayName) || other.displayName == displayName)&&(identical(other.about, about) || other.about == about));
}


@override
int get hashCode => Object.hash(runtimeType,user,displayName,about);

@override
String toString() {
  return 'CoreEvent.profileChanged(user: $user, displayName: $displayName, about: $about)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_ProfileChangedCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_ProfileChangedCopyWith(CoreEvent_ProfileChanged value, $Res Function(CoreEvent_ProfileChanged) _then) = _$CoreEvent_ProfileChangedCopyWithImpl;
@useResult
$Res call({
 UserRef user, String? displayName, String? about
});




}
/// @nodoc
class _$CoreEvent_ProfileChangedCopyWithImpl<$Res>
    implements $CoreEvent_ProfileChangedCopyWith<$Res> {
  _$CoreEvent_ProfileChangedCopyWithImpl(this._self, this._then);

  final CoreEvent_ProfileChanged _self;
  final $Res Function(CoreEvent_ProfileChanged) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? user = null,Object? displayName = freezed,Object? about = freezed,}) {
  return _then(CoreEvent_ProfileChanged(
user: null == user ? _self.user : user // ignore: cast_nullable_to_non_nullable
as UserRef,displayName: freezed == displayName ? _self.displayName : displayName // ignore: cast_nullable_to_non_nullable
as String?,about: freezed == about ? _self.about : about // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}


}

/// @nodoc


class CoreEvent_GroupSettingsChanged extends CoreEvent {
  const CoreEvent_GroupSettingsChanged({this.name, this.previous, required this.by}): super._();
  

/// The group's name, if set.
 final  String? name;
/// The name stored before; unset on this device's first sync, when
/// no rename notice is shown (app-ui: Notices in the chat).
 final  String? previous;
/// The user whose admin device renamed it.
 final  UserRef by;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_GroupSettingsChangedCopyWith<CoreEvent_GroupSettingsChanged> get copyWith => _$CoreEvent_GroupSettingsChangedCopyWithImpl<CoreEvent_GroupSettingsChanged>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_GroupSettingsChanged&&(identical(other.name, name) || other.name == name)&&(identical(other.previous, previous) || other.previous == previous)&&(identical(other.by, by) || other.by == by));
}


@override
int get hashCode => Object.hash(runtimeType,name,previous,by);

@override
String toString() {
  return 'CoreEvent.groupSettingsChanged(name: $name, previous: $previous, by: $by)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_GroupSettingsChangedCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_GroupSettingsChangedCopyWith(CoreEvent_GroupSettingsChanged value, $Res Function(CoreEvent_GroupSettingsChanged) _then) = _$CoreEvent_GroupSettingsChangedCopyWithImpl;
@useResult
$Res call({
 String? name, String? previous, UserRef by
});




}
/// @nodoc
class _$CoreEvent_GroupSettingsChangedCopyWithImpl<$Res>
    implements $CoreEvent_GroupSettingsChangedCopyWith<$Res> {
  _$CoreEvent_GroupSettingsChangedCopyWithImpl(this._self, this._then);

  final CoreEvent_GroupSettingsChanged _self;
  final $Res Function(CoreEvent_GroupSettingsChanged) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? name = freezed,Object? previous = freezed,Object? by = null,}) {
  return _then(CoreEvent_GroupSettingsChanged(
name: freezed == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String?,previous: freezed == previous ? _self.previous : previous // ignore: cast_nullable_to_non_nullable
as String?,by: null == by ? _self.by : by // ignore: cast_nullable_to_non_nullable
as UserRef,
  ));
}


}

/// @nodoc


class CoreEvent_Unreadable extends CoreEvent {
  const CoreEvent_Unreadable({this.sender, this.acceptedAtMs}): super._();
  

/// Who the Pi says sent it.
 final  UserRef? sender;
/// When the Pi accepted it.
 final  PlatformInt64? acceptedAtMs;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_UnreadableCopyWith<CoreEvent_Unreadable> get copyWith => _$CoreEvent_UnreadableCopyWithImpl<CoreEvent_Unreadable>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_Unreadable&&(identical(other.sender, sender) || other.sender == sender)&&(identical(other.acceptedAtMs, acceptedAtMs) || other.acceptedAtMs == acceptedAtMs));
}


@override
int get hashCode => Object.hash(runtimeType,sender,acceptedAtMs);

@override
String toString() {
  return 'CoreEvent.unreadable(sender: $sender, acceptedAtMs: $acceptedAtMs)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_UnreadableCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_UnreadableCopyWith(CoreEvent_Unreadable value, $Res Function(CoreEvent_Unreadable) _then) = _$CoreEvent_UnreadableCopyWithImpl;
@useResult
$Res call({
 UserRef? sender, PlatformInt64? acceptedAtMs
});




}
/// @nodoc
class _$CoreEvent_UnreadableCopyWithImpl<$Res>
    implements $CoreEvent_UnreadableCopyWith<$Res> {
  _$CoreEvent_UnreadableCopyWithImpl(this._self, this._then);

  final CoreEvent_Unreadable _self;
  final $Res Function(CoreEvent_Unreadable) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? sender = freezed,Object? acceptedAtMs = freezed,}) {
  return _then(CoreEvent_Unreadable(
sender: freezed == sender ? _self.sender : sender // ignore: cast_nullable_to_non_nullable
as UserRef?,acceptedAtMs: freezed == acceptedAtMs ? _self.acceptedAtMs : acceptedAtMs // ignore: cast_nullable_to_non_nullable
as PlatformInt64?,
  ));
}


}

/// @nodoc


class CoreEvent_Gap extends CoreEvent {
  const CoreEvent_Gap({required this.newestExpiredAtMs}): super._();
  

/// `accepted_at` of the newest expired message.
 final  PlatformInt64 newestExpiredAtMs;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_GapCopyWith<CoreEvent_Gap> get copyWith => _$CoreEvent_GapCopyWithImpl<CoreEvent_Gap>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_Gap&&(identical(other.newestExpiredAtMs, newestExpiredAtMs) || other.newestExpiredAtMs == newestExpiredAtMs));
}


@override
int get hashCode => Object.hash(runtimeType,newestExpiredAtMs);

@override
String toString() {
  return 'CoreEvent.gap(newestExpiredAtMs: $newestExpiredAtMs)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_GapCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_GapCopyWith(CoreEvent_Gap value, $Res Function(CoreEvent_Gap) _then) = _$CoreEvent_GapCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 newestExpiredAtMs
});




}
/// @nodoc
class _$CoreEvent_GapCopyWithImpl<$Res>
    implements $CoreEvent_GapCopyWith<$Res> {
  _$CoreEvent_GapCopyWithImpl(this._self, this._then);

  final CoreEvent_Gap _self;
  final $Res Function(CoreEvent_Gap) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? newestExpiredAtMs = null,}) {
  return _then(CoreEvent_Gap(
newestExpiredAtMs: null == newestExpiredAtMs ? _self.newestExpiredAtMs : newestExpiredAtMs // ignore: cast_nullable_to_non_nullable
as PlatformInt64,
  ));
}


}

/// @nodoc


class CoreEvent_RemovedForInactivity extends CoreEvent {
  const CoreEvent_RemovedForInactivity({required this.missedSinceMs}): super._();
  

/// Start of the missed period.
 final  PlatformInt64 missedSinceMs;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_RemovedForInactivityCopyWith<CoreEvent_RemovedForInactivity> get copyWith => _$CoreEvent_RemovedForInactivityCopyWithImpl<CoreEvent_RemovedForInactivity>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_RemovedForInactivity&&(identical(other.missedSinceMs, missedSinceMs) || other.missedSinceMs == missedSinceMs));
}


@override
int get hashCode => Object.hash(runtimeType,missedSinceMs);

@override
String toString() {
  return 'CoreEvent.removedForInactivity(missedSinceMs: $missedSinceMs)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_RemovedForInactivityCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_RemovedForInactivityCopyWith(CoreEvent_RemovedForInactivity value, $Res Function(CoreEvent_RemovedForInactivity) _then) = _$CoreEvent_RemovedForInactivityCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 missedSinceMs
});




}
/// @nodoc
class _$CoreEvent_RemovedForInactivityCopyWithImpl<$Res>
    implements $CoreEvent_RemovedForInactivityCopyWith<$Res> {
  _$CoreEvent_RemovedForInactivityCopyWithImpl(this._self, this._then);

  final CoreEvent_RemovedForInactivity _self;
  final $Res Function(CoreEvent_RemovedForInactivity) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? missedSinceMs = null,}) {
  return _then(CoreEvent_RemovedForInactivity(
missedSinceMs: null == missedSinceMs ? _self.missedSinceMs : missedSinceMs // ignore: cast_nullable_to_non_nullable
as PlatformInt64,
  ));
}


}

/// @nodoc


class CoreEvent_Rejoined extends CoreEvent {
  const CoreEvent_Rejoined(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_Rejoined);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreEvent.rejoined()';
}


}




/// @nodoc


class CoreEvent_IdentityKeyChanged extends CoreEvent {
  const CoreEvent_IdentityKeyChanged({required this.user}): super._();
  

/// The user.
 final  UserRef user;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_IdentityKeyChangedCopyWith<CoreEvent_IdentityKeyChanged> get copyWith => _$CoreEvent_IdentityKeyChangedCopyWithImpl<CoreEvent_IdentityKeyChanged>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_IdentityKeyChanged&&(identical(other.user, user) || other.user == user));
}


@override
int get hashCode => Object.hash(runtimeType,user);

@override
String toString() {
  return 'CoreEvent.identityKeyChanged(user: $user)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_IdentityKeyChangedCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_IdentityKeyChangedCopyWith(CoreEvent_IdentityKeyChanged value, $Res Function(CoreEvent_IdentityKeyChanged) _then) = _$CoreEvent_IdentityKeyChangedCopyWithImpl;
@useResult
$Res call({
 UserRef user
});




}
/// @nodoc
class _$CoreEvent_IdentityKeyChangedCopyWithImpl<$Res>
    implements $CoreEvent_IdentityKeyChangedCopyWith<$Res> {
  _$CoreEvent_IdentityKeyChangedCopyWithImpl(this._self, this._then);

  final CoreEvent_IdentityKeyChanged _self;
  final $Res Function(CoreEvent_IdentityKeyChanged) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? user = null,}) {
  return _then(CoreEvent_IdentityKeyChanged(
user: null == user ? _self.user : user // ignore: cast_nullable_to_non_nullable
as UserRef,
  ));
}


}

/// @nodoc


class CoreEvent_ServerIdentityError extends CoreEvent {
  const CoreEvent_ServerIdentityError(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_ServerIdentityError);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreEvent.serverIdentityError()';
}


}




/// @nodoc


class CoreEvent_Removed extends CoreEvent {
  const CoreEvent_Removed(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_Removed);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreEvent.removed()';
}


}




/// @nodoc


class CoreEvent_JoinedGroup extends CoreEvent {
  const CoreEvent_JoinedGroup(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_JoinedGroup);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreEvent.joinedGroup()';
}


}




/// @nodoc


class CoreEvent_MembersChanged extends CoreEvent {
  const CoreEvent_MembersChanged({required final  List<MemberDevice> added, required final  List<MemberDevice> removed, this.by, required final  List<MemberDevice> automatic}): _added = added,_removed = removed,_automatic = automatic,super._();
  

/// Devices added.
 final  List<MemberDevice> _added;
/// Devices added.
 List<MemberDevice> get added {
  if (_added is EqualUnmodifiableListView) return _added;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_added);
}

/// Devices removed.
 final  List<MemberDevice> _removed;
/// Devices removed.
 List<MemberDevice> get removed {
  if (_removed is EqualUnmodifiableListView) return _removed;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_removed);
}

/// The committer's user; unset when this device joined.
 final  UserRef? by;
/// Those of `added` and `removed` that changed automatically
/// (inactivity Removes and re-adds): no notice for them.
 final  List<MemberDevice> _automatic;
/// Those of `added` and `removed` that changed automatically
/// (inactivity Removes and re-adds): no notice for them.
 List<MemberDevice> get automatic {
  if (_automatic is EqualUnmodifiableListView) return _automatic;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_automatic);
}


/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_MembersChangedCopyWith<CoreEvent_MembersChanged> get copyWith => _$CoreEvent_MembersChangedCopyWithImpl<CoreEvent_MembersChanged>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_MembersChanged&&const DeepCollectionEquality().equals(other._added, _added)&&const DeepCollectionEquality().equals(other._removed, _removed)&&(identical(other.by, by) || other.by == by)&&const DeepCollectionEquality().equals(other._automatic, _automatic));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_added),const DeepCollectionEquality().hash(_removed),by,const DeepCollectionEquality().hash(_automatic));

@override
String toString() {
  return 'CoreEvent.membersChanged(added: $added, removed: $removed, by: $by, automatic: $automatic)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_MembersChangedCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_MembersChangedCopyWith(CoreEvent_MembersChanged value, $Res Function(CoreEvent_MembersChanged) _then) = _$CoreEvent_MembersChangedCopyWithImpl;
@useResult
$Res call({
 List<MemberDevice> added, List<MemberDevice> removed, UserRef? by, List<MemberDevice> automatic
});




}
/// @nodoc
class _$CoreEvent_MembersChangedCopyWithImpl<$Res>
    implements $CoreEvent_MembersChangedCopyWith<$Res> {
  _$CoreEvent_MembersChangedCopyWithImpl(this._self, this._then);

  final CoreEvent_MembersChanged _self;
  final $Res Function(CoreEvent_MembersChanged) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? added = null,Object? removed = null,Object? by = freezed,Object? automatic = null,}) {
  return _then(CoreEvent_MembersChanged(
added: null == added ? _self._added : added // ignore: cast_nullable_to_non_nullable
as List<MemberDevice>,removed: null == removed ? _self._removed : removed // ignore: cast_nullable_to_non_nullable
as List<MemberDevice>,by: freezed == by ? _self.by : by // ignore: cast_nullable_to_non_nullable
as UserRef?,automatic: null == automatic ? _self._automatic : automatic // ignore: cast_nullable_to_non_nullable
as List<MemberDevice>,
  ));
}


}

/// @nodoc


class CoreEvent_CommitRefused extends CoreEvent {
  const CoreEvent_CommitRefused({required this.reason}): super._();
  

/// Why, for display.
 final  String reason;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_CommitRefusedCopyWith<CoreEvent_CommitRefused> get copyWith => _$CoreEvent_CommitRefusedCopyWithImpl<CoreEvent_CommitRefused>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_CommitRefused&&(identical(other.reason, reason) || other.reason == reason));
}


@override
int get hashCode => Object.hash(runtimeType,reason);

@override
String toString() {
  return 'CoreEvent.commitRefused(reason: $reason)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_CommitRefusedCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_CommitRefusedCopyWith(CoreEvent_CommitRefused value, $Res Function(CoreEvent_CommitRefused) _then) = _$CoreEvent_CommitRefusedCopyWithImpl;
@useResult
$Res call({
 String reason
});




}
/// @nodoc
class _$CoreEvent_CommitRefusedCopyWithImpl<$Res>
    implements $CoreEvent_CommitRefusedCopyWith<$Res> {
  _$CoreEvent_CommitRefusedCopyWithImpl(this._self, this._then);

  final CoreEvent_CommitRefused _self;
  final $Res Function(CoreEvent_CommitRefused) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? reason = null,}) {
  return _then(CoreEvent_CommitRefused(
reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CoreEvent_ReaddRequested extends CoreEvent {
  const CoreEvent_ReaddRequested({required this.outcome}): super._();
  

/// What the Pi answered.
 final  ReaddAnswer outcome;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_ReaddRequestedCopyWith<CoreEvent_ReaddRequested> get copyWith => _$CoreEvent_ReaddRequestedCopyWithImpl<CoreEvent_ReaddRequested>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_ReaddRequested&&(identical(other.outcome, outcome) || other.outcome == outcome));
}


@override
int get hashCode => Object.hash(runtimeType,outcome);

@override
String toString() {
  return 'CoreEvent.readdRequested(outcome: $outcome)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_ReaddRequestedCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_ReaddRequestedCopyWith(CoreEvent_ReaddRequested value, $Res Function(CoreEvent_ReaddRequested) _then) = _$CoreEvent_ReaddRequestedCopyWithImpl;
@useResult
$Res call({
 ReaddAnswer outcome
});




}
/// @nodoc
class _$CoreEvent_ReaddRequestedCopyWithImpl<$Res>
    implements $CoreEvent_ReaddRequestedCopyWith<$Res> {
  _$CoreEvent_ReaddRequestedCopyWithImpl(this._self, this._then);

  final CoreEvent_ReaddRequested _self;
  final $Res Function(CoreEvent_ReaddRequested) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? outcome = null,}) {
  return _then(CoreEvent_ReaddRequested(
outcome: null == outcome ? _self.outcome : outcome // ignore: cast_nullable_to_non_nullable
as ReaddAnswer,
  ));
}


}

/// @nodoc


class CoreEvent_Connection extends CoreEvent {
  const CoreEvent_Connection({required this.status}): super._();
  

/// The new state.
 final  ConnectionStatus status;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEvent_ConnectionCopyWith<CoreEvent_Connection> get copyWith => _$CoreEvent_ConnectionCopyWithImpl<CoreEvent_Connection>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_Connection&&(identical(other.status, status) || other.status == status));
}


@override
int get hashCode => Object.hash(runtimeType,status);

@override
String toString() {
  return 'CoreEvent.connection(status: $status)';
}


}

/// @nodoc
abstract mixin class $CoreEvent_ConnectionCopyWith<$Res> implements $CoreEventCopyWith<$Res> {
  factory $CoreEvent_ConnectionCopyWith(CoreEvent_Connection value, $Res Function(CoreEvent_Connection) _then) = _$CoreEvent_ConnectionCopyWithImpl;
@useResult
$Res call({
 ConnectionStatus status
});




}
/// @nodoc
class _$CoreEvent_ConnectionCopyWithImpl<$Res>
    implements $CoreEvent_ConnectionCopyWith<$Res> {
  _$CoreEvent_ConnectionCopyWithImpl(this._self, this._then);

  final CoreEvent_Connection _self;
  final $Res Function(CoreEvent_Connection) _then;

/// Create a copy of CoreEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? status = null,}) {
  return _then(CoreEvent_Connection(
status: null == status ? _self.status : status // ignore: cast_nullable_to_non_nullable
as ConnectionStatus,
  ));
}


}

/// @nodoc


class CoreEvent_Other extends CoreEvent {
  const CoreEvent_Other(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEvent_Other);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreEvent.other()';
}


}




/// @nodoc
mixin _$MentionTarget {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MentionTarget);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MentionTarget()';
}


}

/// @nodoc
class $MentionTargetCopyWith<$Res>  {
$MentionTargetCopyWith(MentionTarget _, $Res Function(MentionTarget) __);
}


/// Adds pattern-matching-related methods to [MentionTarget].
extension MentionTargetPatterns on MentionTarget {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( MentionTarget_User value)?  user,TResult Function( MentionTarget_Everyone value)?  everyone,required TResult orElse(),}){
final _that = this;
switch (_that) {
case MentionTarget_User() when user != null:
return user(_that);case MentionTarget_Everyone() when everyone != null:
return everyone(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( MentionTarget_User value)  user,required TResult Function( MentionTarget_Everyone value)  everyone,}){
final _that = this;
switch (_that) {
case MentionTarget_User():
return user(_that);case MentionTarget_Everyone():
return everyone(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( MentionTarget_User value)?  user,TResult? Function( MentionTarget_Everyone value)?  everyone,}){
final _that = this;
switch (_that) {
case MentionTarget_User() when user != null:
return user(_that);case MentionTarget_Everyone() when everyone != null:
return everyone(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( UserRef user)?  user,TResult Function()?  everyone,required TResult orElse(),}) {final _that = this;
switch (_that) {
case MentionTarget_User() when user != null:
return user(_that.user);case MentionTarget_Everyone() when everyone != null:
return everyone();case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( UserRef user)  user,required TResult Function()  everyone,}) {final _that = this;
switch (_that) {
case MentionTarget_User():
return user(_that.user);case MentionTarget_Everyone():
return everyone();}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( UserRef user)?  user,TResult? Function()?  everyone,}) {final _that = this;
switch (_that) {
case MentionTarget_User() when user != null:
return user(_that.user);case MentionTarget_Everyone() when everyone != null:
return everyone();case _:
  return null;

}
}

}

/// @nodoc


class MentionTarget_User extends MentionTarget {
  const MentionTarget_User({required this.user}): super._();
  

/// The user.
 final  UserRef user;

/// Create a copy of MentionTarget
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MentionTarget_UserCopyWith<MentionTarget_User> get copyWith => _$MentionTarget_UserCopyWithImpl<MentionTarget_User>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MentionTarget_User&&(identical(other.user, user) || other.user == user));
}


@override
int get hashCode => Object.hash(runtimeType,user);

@override
String toString() {
  return 'MentionTarget.user(user: $user)';
}


}

/// @nodoc
abstract mixin class $MentionTarget_UserCopyWith<$Res> implements $MentionTargetCopyWith<$Res> {
  factory $MentionTarget_UserCopyWith(MentionTarget_User value, $Res Function(MentionTarget_User) _then) = _$MentionTarget_UserCopyWithImpl;
@useResult
$Res call({
 UserRef user
});




}
/// @nodoc
class _$MentionTarget_UserCopyWithImpl<$Res>
    implements $MentionTarget_UserCopyWith<$Res> {
  _$MentionTarget_UserCopyWithImpl(this._self, this._then);

  final MentionTarget_User _self;
  final $Res Function(MentionTarget_User) _then;

/// Create a copy of MentionTarget
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? user = null,}) {
  return _then(MentionTarget_User(
user: null == user ? _self.user : user // ignore: cast_nullable_to_non_nullable
as UserRef,
  ));
}


}

/// @nodoc


class MentionTarget_Everyone extends MentionTarget {
  const MentionTarget_Everyone(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MentionTarget_Everyone);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MentionTarget.everyone()';
}


}




/// @nodoc
mixin _$SendState {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SendState);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SendState()';
}


}

/// @nodoc
class $SendStateCopyWith<$Res>  {
$SendStateCopyWith(SendState _, $Res Function(SendState) __);
}


/// Adds pattern-matching-related methods to [SendState].
extension SendStatePatterns on SendState {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( SendState_Sending value)?  sending,TResult Function( SendState_Accepted value)?  accepted,TResult Function( SendState_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case SendState_Sending() when sending != null:
return sending(_that);case SendState_Accepted() when accepted != null:
return accepted(_that);case SendState_Failed() when failed != null:
return failed(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( SendState_Sending value)  sending,required TResult Function( SendState_Accepted value)  accepted,required TResult Function( SendState_Failed value)  failed,}){
final _that = this;
switch (_that) {
case SendState_Sending():
return sending(_that);case SendState_Accepted():
return accepted(_that);case SendState_Failed():
return failed(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( SendState_Sending value)?  sending,TResult? Function( SendState_Accepted value)?  accepted,TResult? Function( SendState_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case SendState_Sending() when sending != null:
return sending(_that);case SendState_Accepted() when accepted != null:
return accepted(_that);case SendState_Failed() when failed != null:
return failed(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  sending,TResult Function()?  accepted,TResult Function( String reason)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case SendState_Sending() when sending != null:
return sending();case SendState_Accepted() when accepted != null:
return accepted();case SendState_Failed() when failed != null:
return failed(_that.reason);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  sending,required TResult Function()  accepted,required TResult Function( String reason)  failed,}) {final _that = this;
switch (_that) {
case SendState_Sending():
return sending();case SendState_Accepted():
return accepted();case SendState_Failed():
return failed(_that.reason);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  sending,TResult? Function()?  accepted,TResult? Function( String reason)?  failed,}) {final _that = this;
switch (_that) {
case SendState_Sending() when sending != null:
return sending();case SendState_Accepted() when accepted != null:
return accepted();case SendState_Failed() when failed != null:
return failed(_that.reason);case _:
  return null;

}
}

}

/// @nodoc


class SendState_Sending extends SendState {
  const SendState_Sending(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SendState_Sending);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SendState.sending()';
}


}




/// @nodoc


class SendState_Accepted extends SendState {
  const SendState_Accepted(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SendState_Accepted);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SendState.accepted()';
}


}




/// @nodoc


class SendState_Failed extends SendState {
  const SendState_Failed({required this.reason}): super._();
  

/// Why, for display.
 final  String reason;

/// Create a copy of SendState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SendState_FailedCopyWith<SendState_Failed> get copyWith => _$SendState_FailedCopyWithImpl<SendState_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SendState_Failed&&(identical(other.reason, reason) || other.reason == reason));
}


@override
int get hashCode => Object.hash(runtimeType,reason);

@override
String toString() {
  return 'SendState.failed(reason: $reason)';
}


}

/// @nodoc
abstract mixin class $SendState_FailedCopyWith<$Res> implements $SendStateCopyWith<$Res> {
  factory $SendState_FailedCopyWith(SendState_Failed value, $Res Function(SendState_Failed) _then) = _$SendState_FailedCopyWithImpl;
@useResult
$Res call({
 String reason
});




}
/// @nodoc
class _$SendState_FailedCopyWithImpl<$Res>
    implements $SendState_FailedCopyWith<$Res> {
  _$SendState_FailedCopyWithImpl(this._self, this._then);

  final SendState_Failed _self;
  final $Res Function(SendState_Failed) _then;

/// Create a copy of SendState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? reason = null,}) {
  return _then(SendState_Failed(
reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
