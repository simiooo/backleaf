// OLE callbacks never enter a borrowed GPUI view. Data extraction runs on a separate STA.
#define UNICODE
#define _UNICODE
#define NOMINMAX
#include <windows.h>
#include <ole2.h>
#include <shlobj.h>
#include <shldisp.h>
#include <atomic>
#include <array>
#include <memory>
#include <mutex>
#include <thread>

extern "C" {
struct Callbacks {
 void (*offer)(void*,const unsigned char*,unsigned long long);
 void (*import_data)(void*,void*,unsigned long long);
 unsigned long (*over)(void*,int,int,unsigned long,unsigned long);
 void (*leave)(void*);
 void* (*prepare)(void*,unsigned long);
 long (*execute)(void*,unsigned long*);
 void (*discard)(void*);
 void (*release)(void*);
 int (*ready)(void*);
};
typedef long (*RenderFiles)(void*,void**);
typedef void (*ReleaseFiles)(void*,long,unsigned long);
typedef void (*CancelFiles)(void*);
}
template<class F> static bool background(F f) noexcept {try{std::thread(std::move(f)).detach();return true;}catch(...){return false;}}
static std::atomic<unsigned> imports{0};
static void releaseStream(IStream* stream){if(stream){CoReleaseMarshalData(stream);stream->Release();}}
static UINT tokenFormat(){static UINT f=RegisterClipboardFormatW(L"Backleaf.FrozenSelection.v1");return f;}
static UINT performedFormat(){static UINT f=RegisterClipboardFormatW(CFSTR_PERFORMEDDROPEFFECT);return f;}
static UINT logicalFormat(){static UINT f=RegisterClipboardFormatW(CFSTR_LOGICALPERFORMEDDROPEFFECT);return f;}
static void pump(){MSG m;while(PeekMessageW(&m,nullptr,0,0,PM_REMOVE)){if(m.message==WM_QUIT){PostQuitMessage((int)m.wParam);break;}TranslateMessage(&m);DispatchMessageW(&m);}}
static void pumpUi(){MSG m;while(PeekMessageW(&m,nullptr,0,0,PM_REMOVE|PM_QS_POSTMESSAGE|PM_QS_PAINT)){if(m.message==WM_QUIT){PostQuitMessage((int)m.wParam);break;}TranslateMessage(&m);DispatchMessageW(&m);}}
static FORMATETC format(UINT f){return {(CLIPFORMAT)f,nullptr,DVASPECT_CONTENT,-1,TYMED_HGLOBAL};}
static void setEffect(IDataObject*o,UINT f,DWORD e){HGLOBAL h=GlobalAlloc(GMEM_MOVEABLE,4);if(!h)return;void*p=GlobalLock(h);if(!p){GlobalFree(h);return;}memcpy(p,&e,4);GlobalUnlock(h);FORMATETC t=format(f);STGMEDIUM s{};s.tymed=TYMED_HGLOBAL;s.hGlobal=h;if(FAILED(o->SetData(&t,&s,TRUE)))GlobalFree(h);}
struct Job;
struct Site {
 HWND owner=nullptr,dispatch=nullptr;void*ctx=nullptr;Callbacks cb{};
 std::atomic<bool>alive{true};std::atomic<unsigned long long>generation{0};
 std::mutex mutex;std::shared_ptr<Job> source;
 ~Site(){if(cb.release)cb.release(ctx);}
};
struct SourceObject: IDataObject,IDataObjectAsyncCapability {
 std::atomic<ULONG> refs{1};void*ctx;RenderFiles render;std::array<unsigned char,16> token;
 std::atomic<bool>async{true},operation{false},rendering{false};std::atomic<DWORD>logical{0};
 SourceObject(void*c,RenderFiles r,const unsigned char*t):ctx(c),render(r){memcpy(token.data(),t,16);}
 HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void**p)override{if(!p)return E_POINTER;*p=nullptr;if(id==IID_IUnknown||id==IID_IDataObject)*p=static_cast<IDataObject*>(this);else if(id==IID_IDataObjectAsyncCapability)*p=static_cast<IDataObjectAsyncCapability*>(this);else return E_NOINTERFACE;AddRef();return S_OK;}
 ULONG STDMETHODCALLTYPE AddRef()override{return ++refs;}ULONG STDMETHODCALLTYPE Release()override{auto n=--refs;if(!n)delete this;return n;}
 HRESULT STDMETHODCALLTYPE QueryGetData(FORMATETC*f)override{return f&&f->dwAspect==DVASPECT_CONTENT&&(f->tymed&TYMED_HGLOBAL)&&f->lindex==-1&&(f->cfFormat==CF_HDROP||f->cfFormat==tokenFormat())?S_OK:DV_E_FORMATETC;}
 HRESULT STDMETHODCALLTYPE GetData(FORMATETC*f,STGMEDIUM*s)override{
  if(!s)return E_POINTER;ZeroMemory(s,sizeof(*s));if(QueryGetData(f)!=S_OK)return DV_E_FORMATETC;
  HGLOBAL h=nullptr;if(f->cfFormat==tokenFormat()){h=GlobalAlloc(GMEM_MOVEABLE,16);if(!h)return E_OUTOFMEMORY;void*p=GlobalLock(h);if(!p){GlobalFree(h);return E_OUTOFMEMORY;}memcpy(p,token.data(),16);GlobalUnlock(h);}
  else{rendering=true;HRESULT hr=render(ctx,(void**)&h);rendering=false;if(FAILED(hr))return hr;if(!h)return E_FAIL;}
  s->tymed=TYMED_HGLOBAL;s->hGlobal=h;return S_OK;
 }
 HRESULT STDMETHODCALLTYPE GetDataHere(FORMATETC*,STGMEDIUM*)override{return DATA_E_FORMATETC;}
 HRESULT STDMETHODCALLTYPE GetCanonicalFormatEtc(FORMATETC*,FORMATETC*f)override{if(f)f->ptd=nullptr;return E_NOTIMPL;}
 HRESULT STDMETHODCALLTYPE SetData(FORMATETC*f,STGMEDIUM*s,BOOL release)override{
  if(!f||!s)return E_POINTER;if(s->tymed==TYMED_HGLOBAL&&s->hGlobal&&GlobalSize(s->hGlobal)>=4&&f->cfFormat==logicalFormat()){
   void*p=GlobalLock(s->hGlobal);if(p){logical=*(DWORD*)p;GlobalUnlock(s->hGlobal);}}
  if(release)ReleaseStgMedium(s);return S_OK;
 }
 HRESULT STDMETHODCALLTYPE EnumFormatEtc(DWORD direction,IEnumFORMATETC**p)override{if(direction!=DATADIR_GET)return E_NOTIMPL;FORMATETC f[]={format(tokenFormat()),format(CF_HDROP)};return SHCreateStdEnumFmtEtc(2,f,p);}
 HRESULT STDMETHODCALLTYPE DAdvise(FORMATETC*,DWORD,IAdviseSink*,DWORD*)override{return OLE_E_ADVISENOTSUPPORTED;}
 HRESULT STDMETHODCALLTYPE DUnadvise(DWORD)override{return OLE_E_ADVISENOTSUPPORTED;}
 HRESULT STDMETHODCALLTYPE EnumDAdvise(IEnumSTATDATA**)override{return OLE_E_ADVISENOTSUPPORTED;}
 HRESULT STDMETHODCALLTYPE SetAsyncMode(BOOL b)override{async=b!=FALSE;return S_OK;}
 HRESULT STDMETHODCALLTYPE GetAsyncMode(BOOL*b)override{if(!b)return E_POINTER;*b=async;return S_OK;}
 HRESULT STDMETHODCALLTYPE StartOperation(IBindCtx*)override{operation=true;return S_OK;}
 HRESULT STDMETHODCALLTYPE InOperation(BOOL*b)override{if(!b)return E_POINTER;*b=operation;return S_OK;}
 HRESULT STDMETHODCALLTYPE EndOperation(HRESULT,IBindCtx*,DWORD effect)override{if(effect)logical=effect;operation=false;return S_OK;}
};
struct Job {std::shared_ptr<Site>site;void*ctx=nullptr;RenderFiles render=nullptr;ReleaseFiles release=nullptr;CancelFiles cancel=nullptr;std::array<unsigned char,16>token{};IStream*stream=nullptr;std::atomic<bool>done{false},loop{true},rendering{false};HRESULT hr=DRAGDROP_S_CANCEL;DWORD effect=0;};
struct DropSource:IDropSource{
 std::atomic<ULONG>refs{1};std::shared_ptr<Site>site;DropSource(std::shared_ptr<Site>s):site(s){}
 HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void**p)override{if(!p)return E_POINTER;*p=nullptr;if(id!=IID_IUnknown&&id!=IID_IDropSource)return E_NOINTERFACE;*p=this;AddRef();return S_OK;}
 ULONG STDMETHODCALLTYPE AddRef()override{return ++refs;}ULONG STDMETHODCALLTYPE Release()override{auto n=--refs;if(!n)delete this;return n;}
 HRESULT STDMETHODCALLTYPE QueryContinueDrag(BOOL escape,DWORD)override{if(escape||!site->alive||(GetAsyncKeyState(VK_ESCAPE)&0x8000))return DRAGDROP_S_CANCEL;return GetAsyncKeyState(VK_LBUTTON)&0x8000?S_OK:DRAGDROP_S_DROP;}
 HRESULT STDMETHODCALLTYPE GiveFeedback(DWORD)override{return DRAGDROP_S_USEDEFAULTCURSORS;}
};
static LRESULT CALLBACK dispatch(HWND h,UINT m,WPARAM w,LPARAM l){
 if(m==WM_NCCREATE)SetWindowLongPtrW(h,GWLP_USERDATA,(LONG_PTR)((CREATESTRUCTW*)l)->lpCreateParams);
 auto*site=(Site*)GetWindowLongPtrW(h,GWLP_USERDATA);
 if(m==WM_TIMER&&w==49){KillTimer(h,49);PostMessageW(h,WM_APP+49,0,0);return 0;}
 if(m==WM_APP+49&&site){
  if(site->alive&&!site->cb.ready(site->ctx)){SetTimer(h,49,2,nullptr);return 0;}
  std::shared_ptr<Job>job;{std::lock_guard lock(site->mutex);job=site->source;}
  if(!job)return 0;IDataObject*object=nullptr;IStream*stream=nullptr;{std::lock_guard lock(site->mutex);stream=job->stream;job->stream=nullptr;}
  if(stream&&SUCCEEDED(CoGetInterfaceAndReleaseStream(stream,IID_PPV_ARGS(&object)))){
   if(site->alive&&(GetAsyncKeyState(VK_LBUTTON)&0x8000)){ReleaseCapture();auto*source=new DropSource(job->site);job->hr=DoDragDrop(object,source,DROPEFFECT_COPY|DROPEFFECT_MOVE,&job->effect);source->Release();}else job->hr=S_FALSE;
   object->Release();
  }
  if(job->hr!=DRAGDROP_S_DROP&&job->cancel)job->cancel(job->ctx);job->loop=false;job->done=true;return 0;
 }
 return DefWindowProcW(h,m,w,l);
}
struct Target:IDropTarget{
 std::atomic<ULONG>refs{1};std::shared_ptr<Site>site;IUnknown*entered=nullptr;bool supported=false;DWORD offered=0;
 Target(std::shared_ptr<Site>s):site(s){}~Target(){if(entered)entered->Release();}
 HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void**p)override{if(!p)return E_POINTER;*p=nullptr;if(id!=IID_IUnknown&&id!=IID_IDropTarget)return E_NOINTERFACE;*p=this;AddRef();return S_OK;}
 ULONG STDMETHODCALLTYPE AddRef()override{return ++refs;}ULONG STDMETHODCALLTYPE Release()override{auto n=--refs;if(!n)delete this;return n;}
 DWORD over(DWORD keys,POINTL p,DWORD allowed){POINT point{p.x,p.y};ScreenToClient(site->owner,&point);return site->alive?site->cb.over(site->ctx,point.x,point.y,keys,allowed):0;}
 HRESULT STDMETHODCALLTYPE DragEnter(IDataObject*o,DWORD keys,POINTL p,DWORD*effect)override{
  if(!effect||!o)return E_POINTER;if(entered){entered->Release();entered=nullptr;}o->QueryInterface(IID_PPV_ARGS(&entered));offered=*effect;supported=false;
  auto generation=++site->generation;std::array<unsigned char,16>token{};FORMATETC t=format(tokenFormat());STGMEDIUM medium{};
  if(o->QueryGetData(&t)==S_OK&&SUCCEEDED(o->GetData(&t,&medium))){if(medium.tymed==TYMED_HGLOBAL&&GlobalSize(medium.hGlobal)==16){void*q=GlobalLock(medium.hGlobal);if(q){memcpy(token.data(),q,16);GlobalUnlock(medium.hGlobal);}}ReleaseStgMedium(&medium);}
  t=format(CF_HDROP);supported=o->QueryGetData(&t)==S_OK;
  site->cb.offer(site->ctx,token.data(),generation);
  if(supported&&imports.fetch_add(1)<4){IStream*stream=nullptr;
   if(SUCCEEDED(CoMarshalInterThreadInterfaceInStream(IID_IDataObject,o,&stream))){auto keep=site;
    if(!background([keep,stream,generation]{if(SUCCEEDED(OleInitialize(nullptr))){IDataObject*data=nullptr;if(SUCCEEDED(CoGetInterfaceAndReleaseStream(stream,IID_PPV_ARGS(&data)))){if(keep->alive&&keep->generation==generation)keep->cb.import_data(keep->ctx,data,generation);data->Release();}OleUninitialize();}else stream->Release();--imports;})){releaseStream(stream);--imports;}
   }else --imports;
  }else if(supported)--imports;
  *effect=over(keys,p,offered);
  // A short bounded admission window lets queued GPUI updates and background
  // identity validation run before an immediately following button release.
  for(int i=0;!*effect&&supported&&site->alive&&i<12;i++){pumpUi();Sleep(2);*effect=over(keys,p,offered);}
  return S_OK;
 }
 HRESULT STDMETHODCALLTYPE DragOver(DWORD keys,POINTL p,DWORD*effect)override{if(!effect)return E_POINTER;*effect=supported?over(keys,p,offered):0;return S_OK;}
 HRESULT STDMETHODCALLTYPE DragLeave()override{++site->generation;site->cb.leave(site->ctx);if(entered){entered->Release();entered=nullptr;}return S_OK;}
 HRESULT STDMETHODCALLTYPE Drop(IDataObject*o,DWORD keys,POINTL p,DWORD*effect)override{
  if(!effect||!o)return E_POINTER;IUnknown*identity=nullptr;o->QueryInterface(IID_PPV_ARGS(&identity));bool same=identity&&identity==entered;if(identity)identity->Release();
  DWORD intent=same&&supported?over(keys,p,offered):0;*effect=0;void*job=intent?site->cb.prepare(site->ctx,intent):nullptr;if(!job){DragLeave();return S_OK;}
  IDataObjectAsyncCapability*async=nullptr;BOOL mode=FALSE;bool asynchronous=SUCCEEDED(o->QueryInterface(IID_PPV_ARGS(&async)))&&SUCCEEDED(async->GetAsyncMode(&mode))&&mode&&SUCCEEDED(async->StartOperation(nullptr));if(async)async->Release();
  IStream*stream=nullptr;HRESULT hr=CoMarshalInterThreadInterfaceInStream(IID_IDataObject,o,&stream);
  struct Completion{std::atomic<bool>done{false};HRESULT hr=E_FAIL;DWORD actual=0;};auto completion=std::make_shared<Completion>();auto keep=site;
  if(SUCCEEDED(hr)){
   bool started=background([keep,stream,job,completion,asynchronous,intent]{
    HRESULT init=OleInitialize(nullptr);IDataObject*data=nullptr;HRESULT status=FAILED(init)?init:CoGetInterfaceAndReleaseStream(stream,IID_PPV_ARGS(&data));
    if(FAILED(init))stream->Release();
    DWORD actual=0;HRESULT operation=status;if(SUCCEEDED(status))operation=keep->cb.execute(job,&actual);else keep->cb.discard(job);
    if(data){setEffect(data,performedFormat(),intent==DROPEFFECT_MOVE?DROPEFFECT_NONE:actual);setEffect(data,logicalFormat(),actual);if(asynchronous){IDataObjectAsyncCapability*a=nullptr;if(SUCCEEDED(data->QueryInterface(IID_PPV_ARGS(&a)))){a->EndOperation(operation,nullptr,actual);a->Release();}}data->Release();}
    if(SUCCEEDED(init))OleUninitialize();completion->hr=operation;completion->actual=actual;completion->done=true;
   });
   if(!started){releaseStream(stream);hr=E_OUTOFMEMORY;}
  }
  if(FAILED(hr)){keep->cb.discard(job);if(asynchronous){IDataObjectAsyncCapability*a=nullptr;if(SUCCEEDED(o->QueryInterface(IID_PPV_ARGS(&a)))){a->EndOperation(hr,nullptr,0);a->Release();}}completion->hr=hr;completion->done=true;}
  if(asynchronous)*effect=intent;else{while(!completion->done){pump();Sleep(5);}*effect=intent==DROPEFFECT_MOVE?DROPEFFECT_NONE:completion->actual;}
  DragLeave();return S_OK;
 }
};
extern "C" void* backleaf_ole_install(HWND owner,void*ctx,Callbacks callbacks){
 bool owned=false;try{auto site=std::make_shared<Site>();site->owner=owner;site->ctx=ctx;site->cb=callbacks;owned=true;WNDCLASSW wc{};wc.lpfnWndProc=dispatch;wc.hInstance=GetModuleHandleW(nullptr);wc.lpszClassName=L"BackleafOleDispatch";RegisterClassW(&wc);
 site->dispatch=CreateWindowExW(0,wc.lpszClassName,L"",0,0,0,0,0,HWND_MESSAGE,nullptr,wc.hInstance,site.get());if(!site->dispatch)return nullptr;
 RevokeDragDrop(owner);auto*target=new Target(site);HRESULT hr=RegisterDragDrop(owner,target);target->Release();if(FAILED(hr)){DestroyWindow(site->dispatch);return nullptr;}return new std::shared_ptr<Site>(site);
 }catch(...){if(!owned&&callbacks.release)callbacks.release(ctx);return nullptr;}
}
extern "C" void backleaf_ole_detach(void*opaque){if(!opaque)return;auto*handle=(std::shared_ptr<Site>*)opaque;auto site=*handle;site->alive=false;++site->generation;RevokeDragDrop(site->owner);DestroyWindow(site->dispatch);delete handle;}
extern "C" int backleaf_ole_source(void*opaque,void*ctx,const unsigned char*token,RenderFiles render,ReleaseFiles release,CancelFiles cancel){
 try{if(!opaque)return 0;auto site=*(std::shared_ptr<Site>*)opaque;if(!site->alive)return 0;auto job=std::make_shared<Job>();job->site=site;job->ctx=ctx;job->render=render;job->release=release;job->cancel=cancel;memcpy(job->token.data(),token,16);
 {std::lock_guard lock(site->mutex);if(site->source&&!site->source->done)return 0;site->source=job;}
 if(!background([job]{HRESULT init=OleInitialize(nullptr);SourceObject*object=nullptr;if(SUCCEEDED(init)){object=new SourceObject(job->ctx,job->render,job->token.data());IStream*stream=nullptr;HRESULT hr=CoMarshalInterThreadInterfaceInStream(IID_IDataObject,static_cast<IDataObject*>(object),&stream);if(SUCCEEDED(hr)){std::lock_guard lock(job->site->mutex);job->stream=stream;if(!PostMessageW(job->site->dispatch,WM_APP+49,0,0)){CoReleaseMarshalData(stream);stream->Release();job->stream=nullptr;job->done=true;}}else{job->hr=hr;job->done=true;}
  while(!job->done||object->operation){if(!job->site->alive&&!job->done){job->hr=DRAGDROP_S_CANCEL;job->done=true;}pump();Sleep(5);}{std::lock_guard lock(job->site->mutex);if(job->stream){releaseStream(job->stream);job->stream=nullptr;}}if(object->logical)job->effect=object->logical;object->Release();OleUninitialize();}else{job->hr=init;job->done=true;}
  {std::lock_guard lock(job->site->mutex);if(job->site->source==job)job->site->source.reset();}
  job->release(job->ctx,job->hr,job->effect);
 })){std::lock_guard lock(site->mutex);site->source.reset();return 0;}return 1;}catch(...){return 0;}
}
extern "C" int backleaf_ole_source_busy(void*opaque){if(!opaque)return 0;auto site=*(std::shared_ptr<Site>*)opaque;std::lock_guard lock(site->mutex);return site->source&&!site->source->done;}

// In-process COM protocol probes use the production target, without injecting input.
extern "C" void* backleaf_ole_probe_target(void*opaque){try{return new Target(*(std::shared_ptr<Site>*)opaque);}catch(...){return nullptr;}}
extern "C" long backleaf_ole_probe_step(void*target,void*object,unsigned step,unsigned long*effect){auto*t=(Target*)target;auto*o=(IDataObject*)object;POINTL p{0,0};switch(step){case 0:return t->DragEnter(o,MK_LBUTTON,p,effect);case 1:return t->DragOver(MK_LBUTTON,p,effect);case 2:return t->Drop(o,0,p,effect);default:return t->DragLeave();}}
extern "C" void backleaf_ole_probe_release(void*target){((Target*)target)->Release();}
extern "C" void backleaf_ole_probe_pump(){pumpUi();}
